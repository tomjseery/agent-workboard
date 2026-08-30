use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use workboard_application::AppError;
use workboard_core::{
    CheckoutAvailability, ConversationId, HierarchyOwner, LaunchProfile, LaunchProfileSource,
    ManagedSessionRole, PRODUCT_NAME, ReasoningEffort, RepositoryId, Resumability, Tool, WorkItem,
    WorkItemStatus, WorkflowState, WorkspaceSnapshot,
};

use crate::selector::{RankedCandidate, SelectionCandidate, SelectionResult, resolve};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardControl {
    Continue,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOption {
    pub session_id: Option<ConversationId>,
    pub writer_reservation_key: Option<String>,
    pub repository_id: RepositoryId,
    pub provider: Tool,
    pub profile: LaunchProfile,
    pub role: ManagedSessionRole,
    pub status: String,
    pub last_activity: Option<time::OffsetDateTime>,
    pub checkout: PathBuf,
    pub branch: Option<String>,
    pub resumability: Resumability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSelection {
    pub session_id: Option<ConversationId>,
    pub writer_reservation_key: Option<String>,
    pub repository_id: RepositoryId,
    pub provider: Tool,
    pub profile: LaunchProfile,
    pub role: ManagedSessionRole,
}

struct LaunchPickerState {
    title: String,
    options: Vec<LaunchOption>,
    selected: usize,
}

impl LaunchPickerState {
    fn selected(&self) -> Option<&LaunchOption> {
        self.options.get(self.selected)
    }

    fn selected_mut(&mut self) -> Option<&mut LaunchOption> {
        self.options.get_mut(self.selected)
    }

    fn move_down(&mut self) {
        self.selected = self
            .selected
            .saturating_add(1)
            .min(self.options.len().saturating_sub(1));
    }

    fn cycle_model(&mut self) {
        let Some(option) = self.selected_mut() else {
            return;
        };
        let models: &[&str] = match option.provider {
            Tool::Claude => &["sonnet", "opus", "haiku"],
            Tool::Codex => &["gpt-5.6", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"],
        };
        let current = option.profile.model.as_deref();
        let index = models
            .iter()
            .position(|model| Some(*model) == current)
            .map_or(0, |index| index.saturating_add(1) % models.len());
        option.profile.model = Some(models[index].to_owned());
        option.profile.source = LaunchProfileSource::ExplicitOverride;
    }

    fn cycle_effort(&mut self) {
        let Some(option) = self.selected_mut() else {
            return;
        };
        let efforts: &[ReasoningEffort] = match option.provider {
            Tool::Claude => &[
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ],
            Tool::Codex => &[
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
            ],
        };
        let index = efforts
            .iter()
            .position(|effort| Some(*effort) == option.profile.effort)
            .map_or(0, |index| index.saturating_add(1) % efforts.len());
        option.profile.effort = Some(efforts[index]);
        option.profile.source = LaunchProfileSource::ExplicitOverride;
    }

    fn cycle_role(&mut self) {
        let Some(option) = self.selected_mut() else {
            return;
        };
        if option.session_id.is_some() {
            return;
        }
        option.role = match option.role {
            ManagedSessionRole::WorkItemExecution => ManagedSessionRole::Debugging,
            ManagedSessionRole::Debugging => ManagedSessionRole::Review,
            _ => ManagedSessionRole::WorkItemExecution,
        };
        option.profile.role = option.role;
        option.profile.source = LaunchProfileSource::ExplicitOverride;
    }

    fn selection(&self) -> Option<LaunchSelection> {
        self.selected().map(|option| LaunchSelection {
            session_id: option.session_id,
            writer_reservation_key: option.writer_reservation_key.clone(),
            repository_id: option.repository_id,
            provider: option.provider,
            profile: option.profile.clone(),
            role: option.role,
        })
    }
}

pub fn launch_picker(
    title: &str,
    options: Vec<LaunchOption>,
) -> Result<Option<LaunchSelection>, AppError> {
    let mut state = LaunchPickerState {
        title: title.to_owned(),
        options,
        selected: 0,
    };
    let mut terminal = TerminalSession::new()?;
    loop {
        terminal
            .terminal
            .draw(|frame| render_launch_picker(frame, &state))
            .map_err(terminal_error)?;
        if event::poll(Duration::from_millis(100)).map_err(terminal_error)?
            && let Event::Key(key) = event::read().map_err(terminal_error)?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Enter | KeyCode::Char('s') => return Ok(state.selection()),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Up | KeyCode::Char('k') => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Char('m') => state.cycle_model(),
                KeyCode::Char('e') => state.cycle_effort(),
                KeyCode::Char('r') => state.cycle_role(),
                _ => {}
            }
        }
    }
}

fn render_launch_picker(frame: &mut Frame<'_>, state: &LaunchPickerState) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(8),
        Constraint::Length(2),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new("Choose an associated Workboard session or start another managed CLI")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(state.title.clone()),
            ),
        sections[0],
    );
    let items = state
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let marker = if index == state.selected { ">" } else { " " };
            let action = if option.session_id.is_some() {
                "Resume"
            } else {
                "New"
            };
            ListItem::new(format!(
                "{marker} {action} {}  {}  {}  {}",
                tool_title(option.provider),
                option.profile.model.as_deref().unwrap_or("unknown"),
                role_title(option.role),
                option.status,
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Managed CLIs")),
        sections[1],
    );
    let details = state.selected().map_or_else(
        || vec![Line::from("No launch options")],
        |option| {
            vec![
                Line::from(format!("Provider: {}", tool_title(option.provider))),
                Line::from(format!(
                    "Model / effort: {} / {}",
                    option.profile.model.as_deref().unwrap_or("unknown"),
                    option
                        .profile
                        .effort
                        .map_or("unknown", ReasoningEffort::as_str),
                )),
                Line::from(format!("Role: {}", role_title(option.role))),
                Line::from(format!("Status: {}", option.status)),
                Line::from(format!(
                    "Activity: {}",
                    option
                        .last_activity
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_owned())
                )),
                Line::from(format!("Checkout: {}", option.checkout.display())),
                Line::from(format!(
                    "Branch / resumability: {} / {:?}",
                    option.branch.as_deref().unwrap_or("materialize on Start"),
                    option.resumability,
                )),
            ]
        },
    );
    frame.render_widget(
        Paragraph::new(details).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Configuration"),
        ),
        sections[2],
    );
    frame.render_widget(
        Paragraph::new("Up/Down select  m model  e effort  r role  Enter/s Start  Esc cancel"),
        sections[3],
    );
}

fn tool_title(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "Claude",
        Tool::Codex => "Codex",
    }
}

fn role_title(role: ManagedSessionRole) -> &'static str {
    match role {
        ManagedSessionRole::WorkspacePlanning => "workspace planning",
        ManagedSessionRole::EpicNavigation => "Epic navigation",
        ManagedSessionRole::FeaturePlanning => "Feature planning",
        ManagedSessionRole::WorkItemExecution => "execution",
        ManagedSessionRole::Debugging => "debugging",
        ManagedSessionRole::Review => "review",
    }
}

pub struct BoardState {
    snapshot: WorkspaceSnapshot,
    query: String,
    searching: bool,
    selected: usize,
    no_color: bool,
}

impl BoardState {
    pub fn new(snapshot: WorkspaceSnapshot, no_color: bool) -> Self {
        Self {
            snapshot,
            query: String::new(),
            searching: false,
            selected: 0,
            no_color,
        }
    }

    fn visible_work_items(&self) -> Vec<&WorkItem> {
        if self.query.is_empty() {
            return self.snapshot.work_items.iter().collect();
        }
        let candidates = self
            .snapshot
            .work_items
            .iter()
            .map(|item| SelectionCandidate {
                id: item.id.to_string(),
                key: Some(item.key.to_string()),
                label: item.title.clone(),
                metadata: format!(
                    "{} {}",
                    work_item_status(item.status),
                    repository_metadata(item, &self.snapshot)
                ),
            });
        let ids = match resolve(Some(&self.query), candidates) {
            SelectionResult::Empty => Vec::new(),
            SelectionResult::Selected(candidate) => vec![candidate.id],
            SelectionResult::Picker(candidates) => candidates
                .into_iter()
                .map(|candidate| candidate.candidate.id)
                .collect(),
        };
        ids.iter()
            .filter_map(|id| {
                self.snapshot
                    .work_items
                    .iter()
                    .find(|item| item.id.to_string() == *id)
            })
            .collect()
    }

    fn selected_work_item(&self) -> Option<&WorkItem> {
        let items = self.visible_work_items();
        items
            .get(self.selected.min(items.len().saturating_sub(1)))
            .copied()
    }

    fn handle_key(&mut self, key: KeyEvent) -> BoardControl {
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.selected = 0;
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(character);
                    self.selected = 0;
                }
                _ => {}
            }
            return BoardControl::Continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => BoardControl::Exit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                BoardControl::Exit
            }
            KeyCode::Char('/') => {
                self.searching = true;
                BoardControl::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let last = self.visible_work_items().len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(last);
                BoardControl::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                BoardControl::Continue
            }
            KeyCode::Home => {
                self.selected = 0;
                BoardControl::Continue
            }
            KeyCode::End => {
                self.selected = self.visible_work_items().len().saturating_sub(1);
                BoardControl::Continue
            }
            _ => BoardControl::Continue,
        }
    }
}

pub fn run(snapshot: WorkspaceSnapshot) -> Result<(), AppError> {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let mut state = BoardState::new(snapshot, no_color);
    let mut terminal = TerminalSession::new()?;
    loop {
        terminal
            .terminal
            .draw(|frame| render(frame, &state))
            .map_err(terminal_error)?;
        if event::poll(Duration::from_millis(100)).map_err(terminal_error)?
            && let Event::Key(key) = event::read().map_err(terminal_error)?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            && state.handle_key(key) == BoardControl::Exit
        {
            return Ok(());
        }
    }
}

pub fn pick(
    title: &str,
    candidates: Vec<SelectionCandidate>,
) -> Result<Option<SelectionCandidate>, AppError> {
    let mut picker = PickerState {
        title: title.to_owned(),
        candidates,
        query: String::new(),
        selected: 0,
    };
    let mut terminal = TerminalSession::new()?;
    loop {
        terminal
            .terminal
            .draw(|frame| render_picker(frame, &picker))
            .map_err(terminal_error)?;
        if event::poll(Duration::from_millis(100)).map_err(terminal_error)?
            && let Event::Key(key) = event::read().map_err(terminal_error)?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Enter => return Ok(picker.selected_candidate()),
                KeyCode::Down => picker.move_down(),
                KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
                KeyCode::Backspace => {
                    picker.query.pop();
                    picker.selected = 0;
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    picker.query.push(character);
                    picker.selected = 0;
                }
                _ => {}
            }
        }
    }
}

pub fn checklist(
    title: &str,
    candidates: Vec<SelectionCandidate>,
) -> Result<Option<Vec<String>>, AppError> {
    let mut state = ChecklistState {
        title: title.to_owned(),
        selected: vec![true; candidates.len()],
        candidates,
        cursor: 0,
    };
    let mut terminal = TerminalSession::new()?;
    loop {
        terminal
            .terminal
            .draw(|frame| render_checklist(frame, &state))
            .map_err(terminal_error)?;
        if event::poll(Duration::from_millis(100)).map_err(terminal_error)?
            && let Event::Key(key) = event::read().map_err(terminal_error)?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Enter => return Ok(Some(state.selection())),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Up | KeyCode::Char('k') => {
                    state.cursor = state.cursor.saturating_sub(1);
                }
                KeyCode::Char(' ') => state.toggle(),
                KeyCode::Char('a') => state.selected.fill(true),
                KeyCode::Char('n') => state.selected.fill(false),
                _ => {}
            }
        }
    }
}

pub fn plain(snapshot: &WorkspaceSnapshot) -> String {
    let mut output = format!(
        "{PRODUCT_NAME}: {}\nRepositories: {}\nEpics: {}\nFeatures: {}\nWork items: {}\n",
        snapshot.workspace.title,
        snapshot.repositories.len(),
        snapshot.epics.len(),
        snapshot.features.len(),
        snapshot.work_items.len()
    );
    for warning in warnings(snapshot) {
        output.push_str(&format!("Warning: {warning}\n"));
    }
    for feature in &snapshot.features {
        output.push_str(&format!("\n{}\n", feature.title));
        for item in snapshot
            .work_items
            .iter()
            .filter(|item| item.feature_id == feature.id)
        {
            output.push_str(&format!(
                "  [{}] {} — {}\n",
                work_item_status(item.status),
                item.key,
                item.title
            ));
        }
    }
    output
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &BoardState) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(frame.area());
    render_header(frame, sections[0], state);
    let columns = Layout::horizontal([
        Constraint::Percentage(24),
        Constraint::Percentage(46),
        Constraint::Percentage(30),
    ])
    .split(sections[1]);
    render_hierarchy(frame, columns[0], state);
    render_board(frame, columns[1], state);
    render_details(frame, columns[2], state);
    render_footer(frame, sections[2], state);
}

struct PickerState {
    title: String,
    candidates: Vec<SelectionCandidate>,
    query: String,
    selected: usize,
}

struct ChecklistState {
    title: String,
    candidates: Vec<SelectionCandidate>,
    selected: Vec<bool>,
    cursor: usize,
}

impl ChecklistState {
    fn move_down(&mut self) {
        self.cursor = self
            .cursor
            .saturating_add(1)
            .min(self.candidates.len().saturating_sub(1));
    }

    fn toggle(&mut self) {
        if let Some(selected) = self.selected.get_mut(self.cursor) {
            *selected = !*selected;
        }
    }

    fn selection(&self) -> Vec<String> {
        self.candidates
            .iter()
            .zip(&self.selected)
            .filter(|(_, selected)| **selected)
            .map(|(candidate, _)| candidate.id.clone())
            .collect()
    }
}

impl PickerState {
    fn matches(&self) -> Vec<RankedCandidate> {
        match resolve(Some(&self.query), self.candidates.clone()) {
            SelectionResult::Empty => Vec::new(),
            SelectionResult::Selected(candidate) => vec![RankedCandidate {
                candidate,
                score: u32::MAX,
            }],
            SelectionResult::Picker(candidates) => candidates,
        }
    }

    fn selected_candidate(&self) -> Option<SelectionCandidate> {
        let matches = self.matches();
        matches
            .get(self.selected.min(matches.len().saturating_sub(1)))
            .map(|candidate| candidate.candidate.clone())
    }

    fn move_down(&mut self) {
        self.selected = self
            .selected
            .saturating_add(1)
            .min(self.matches().len().saturating_sub(1));
    }
}

fn render_picker(frame: &mut Frame<'_>, picker: &PickerState) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(format!("Search: {}_", picker.query)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(picker.title.clone()),
        ),
        sections[0],
    );
    let matches = picker.matches();
    let items = if matches.is_empty() {
        vec![ListItem::new("No matches")]
    } else {
        matches
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                let marker = if index == picker.selected { "›" } else { " " };
                ListItem::new(format!(
                    "{marker} {}  {}",
                    candidate.candidate.label,
                    candidate.candidate.key.as_deref().unwrap_or_default()
                ))
            })
            .collect()
    };
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL)),
        sections[1],
    );
    frame.render_widget(
        Paragraph::new("↑/↓ navigate  Enter select  Esc cancel"),
        sections[2],
    );
}

fn render_checklist(frame: &mut Frame<'_>, state: &ChecklistState) {
    let sections =
        Layout::vertical([Constraint::Min(4), Constraint::Length(2)]).split(frame.area());
    let items = state
        .candidates
        .iter()
        .zip(&state.selected)
        .enumerate()
        .map(|(index, (candidate, selected))| {
            let cursor = if index == state.cursor { ">" } else { " " };
            let check = if *selected { "x" } else { " " };
            ListItem::new(format!(
                "{cursor} [{check}] {}  {}",
                candidate.label, candidate.metadata
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(state.title.clone()),
        ),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new("Up/Down navigate  Space toggle  a all  n none  Enter recover  Esc cancel"),
        sections[1],
    );
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &BoardState) {
    let repositories = state
        .snapshot
        .repositories
        .iter()
        .map(|repository| repository.slug.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let title = Line::from(vec![
        styled(
            format!(" {PRODUCT_NAME} "),
            state.no_color,
            Color::Cyan,
            Modifier::BOLD,
        ),
        Span::raw(format!(
            "{}  •  {}",
            state.snapshot.workspace.title, repositories
        )),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_hierarchy(frame: &mut Frame<'_>, area: Rect, state: &BoardState) {
    let mut items = Vec::new();
    for epic in &state.snapshot.epics {
        items.push(ListItem::new(Line::from(vec![
            styled("◆ ", state.no_color, Color::Magenta, Modifier::BOLD),
            Span::raw(epic.title.clone()),
        ])));
        for feature in state
            .snapshot
            .features
            .iter()
            .filter(|feature| feature.epic_id == epic.id)
        {
            items.push(ListItem::new(format!("  ├─ {}", feature.title)));
        }
    }
    if items.is_empty() {
        items.push(ListItem::new("No Epics"));
    }
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Hierarchy")),
        area,
    );
}

fn render_board(frame: &mut Frame<'_>, area: Rect, state: &BoardState) {
    let visible = state.visible_work_items();
    let selected_id = state.selected_work_item().map(|item| item.id);
    let mut lines = Vec::new();
    if visible.is_empty() {
        lines.push(Line::from("No matching Work items"));
    }
    for feature in &state.snapshot.features {
        let feature_items: Vec<_> = visible
            .iter()
            .filter(|item| item.feature_id == feature.id)
            .collect();
        if feature_items.is_empty() {
            continue;
        }
        lines.push(Line::from(styled(
            format!("{}  [{}]", feature.title, workflow_state(feature.state)),
            state.no_color,
            Color::Yellow,
            Modifier::BOLD,
        )));
        for status in board_statuses() {
            let status_items: Vec<_> = feature_items
                .iter()
                .filter(|item| item.status == status)
                .collect();
            if status_items.is_empty() {
                continue;
            }
            lines.push(Line::from(format!("  {}", work_item_status(status))));
            for item in status_items {
                let marker = if selected_id == Some(item.id) {
                    "›"
                } else {
                    " "
                };
                lines.push(Line::from(vec![
                    styled(
                        format!("  {marker} "),
                        state.no_color,
                        Color::Cyan,
                        Modifier::BOLD,
                    ),
                    Span::raw(format!("{} — {}", item.key, item.title)),
                ]));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Work items"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_details(frame: &mut Frame<'_>, area: Rect, state: &BoardState) {
    let Some(item) = state.selected_work_item() else {
        frame.render_widget(
            Paragraph::new("Select a Work item")
                .block(Block::default().borders(Borders::ALL).title("Details")),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::from(styled(
            item.title.clone(),
            state.no_color,
            Color::Cyan,
            Modifier::BOLD,
        )),
        Line::from(item.key.to_string()),
        Line::from(format!("Status: {}", work_item_status(item.status))),
        Line::from(""),
        Line::from("Checkouts"),
    ];
    let effective = state
        .snapshot
        .effective_checkouts
        .iter()
        .filter(|checkout| checkout.work_item_id == Some(item.id));
    let mut checkout_count = 0;
    for effective in effective {
        if let Some(checkout) = state
            .snapshot
            .checkouts
            .iter()
            .find(|checkout| checkout.id == effective.checkout_id)
        {
            checkout_count += 1;
            let mode = if effective.inherited {
                "inherited"
            } else {
                "override"
            };
            lines.push(Line::from(format!(
                "  {} [{}; {mode}]",
                checkout.branch.as_deref().unwrap_or("detached checkout"),
                checkout_availability(checkout.availability)
            )));
            for path in &checkout.paths {
                let interval = if path.observed_until.is_none() {
                    "current"
                } else {
                    "historical"
                };
                lines.push(Line::from(format!(
                    "    {} ({interval})",
                    path.path.display()
                )));
            }
        }
    }
    if checkout_count == 0 {
        lines.push(Line::from("  No effective checkout"));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Sessions"));
    let session_ids: Vec<_> = state
        .snapshot
        .associations
        .iter()
        .filter_map(|association| {
            (association.owner == HierarchyOwner::WorkItem(item.id))
                .then_some(association.session_id)
        })
        .collect();
    if session_ids.is_empty() {
        lines.push(Line::from("  No associated sessions"));
    } else {
        for session in state
            .snapshot
            .sessions
            .iter()
            .filter(|session| session_ids.contains(&session.id))
        {
            lines.push(Line::from(format!(
                "  {} {}",
                session.native.tool(),
                session.id
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Resume • Start • Document • Worktree • Recover • Close",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &BoardState) {
    let mut text = if state.searching {
        format!("Search: {}_", state.query)
    } else if state.query.is_empty() {
        "↑/↓ navigate  / search  q quit".to_owned()
    } else {
        format!("Filter: {}  •  ↑/↓ navigate  / edit  q quit", state.query)
    };
    let warnings = warnings(&state.snapshot);
    if let Some(warning) = warnings.first() {
        text.push_str(&format!("  ⚠ {warning}"));
    }
    frame.render_widget(Paragraph::new(text), area);
}

fn warnings(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    let mut warnings = Vec::new();
    for checkout in &snapshot.checkouts {
        if matches!(
            checkout.availability,
            CheckoutAvailability::Missing | CheckoutAvailability::Deleted
        ) {
            warnings.push(format!(
                "checkout {} is {}",
                checkout.git_worktree_identity,
                checkout_availability(checkout.availability)
            ));
        }
    }
    for feature in &snapshot.features {
        if feature.state == WorkflowState::ReconciliationRequired {
            warnings.push(format!("{} requires reconciliation", feature.title));
        } else if matches!(
            feature.state,
            WorkflowState::WorktreePending
                | WorkflowState::PlanningLaunchPending
                | WorkflowState::Publishing
                | WorkflowState::WorkItemLaunchPending
        ) {
            warnings.push(format!("{} has an interrupted workflow", feature.title));
        }
    }
    warnings
}

fn repository_metadata(item: &WorkItem, snapshot: &WorkspaceSnapshot) -> String {
    item.repository_ids
        .iter()
        .filter_map(|repository_id| {
            snapshot
                .repositories
                .iter()
                .find(|repository| repository.id == *repository_id)
                .map(|repository| repository.title.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn board_statuses() -> [WorkItemStatus; 7] {
    [
        WorkItemStatus::Backlog,
        WorkItemStatus::Ready,
        WorkItemStatus::InProgress,
        WorkItemStatus::Blocked,
        WorkItemStatus::Review,
        WorkItemStatus::Done,
        WorkItemStatus::Cancelled,
    ]
}

fn work_item_status(status: WorkItemStatus) -> &'static str {
    match status {
        WorkItemStatus::Backlog => "Backlog",
        WorkItemStatus::Ready => "Ready",
        WorkItemStatus::InProgress => "In progress",
        WorkItemStatus::Blocked => "Blocked",
        WorkItemStatus::Review => "Review",
        WorkItemStatus::Done => "Done",
        WorkItemStatus::Cancelled => "Cancelled",
    }
}

fn workflow_state(state: WorkflowState) -> &'static str {
    match state {
        WorkflowState::Draft => "Draft",
        WorkflowState::WorktreePending => "Worktree pending",
        WorkflowState::PlanningLaunchPending => "Planning launch pending",
        WorkflowState::PlanningActive => "Planning active",
        WorkflowState::ProposalReady => "Proposal ready",
        WorkflowState::AwaitingApproval => "Awaiting approval",
        WorkflowState::Publishing => "Publishing",
        WorkflowState::Planned => "Planned",
        WorkflowState::WorkItemLaunchPending => "Work-item launch pending",
        WorkflowState::WorkItemActive => "Work-item active",
        WorkflowState::ReconciliationRequired => "Reconciliation required",
        WorkflowState::Blocked => "Blocked",
        WorkflowState::Paused => "Paused",
        WorkflowState::Completed => "Completed",
        WorkflowState::Cancelled => "Cancelled",
    }
}

fn checkout_availability(availability: CheckoutAvailability) -> &'static str {
    match availability {
        CheckoutAvailability::Available => "available",
        CheckoutAvailability::Missing => "missing",
        CheckoutAvailability::Deleted => "deleted",
        CheckoutAvailability::Replaced => "replaced",
    }
}

fn styled(
    content: impl Into<String>,
    no_color: bool,
    color: Color,
    modifier: Modifier,
) -> Span<'static> {
    let content = content.into();
    if no_color {
        Span::raw(content)
    } else {
        Span::styled(content, Style::default().fg(color).add_modifier(modifier))
    }
}

fn terminal_error(source: io::Error) -> AppError {
    AppError::External {
        code: "terminal_io".to_owned(),
        message: source.to_string(),
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn new() -> Result<Self, AppError> {
        enable_raw_mode().map_err(terminal_error)?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            drop(disable_raw_mode());
            return Err(terminal_error(error));
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                drop(disable_raw_mode());
                Err(terminal_error(error))
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        drop(disable_raw_mode());
        drop(execute!(self.terminal.backend_mut(), LeaveAlternateScreen));
        drop(self.terminal.show_cursor());
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use time::OffsetDateTime;
    use workboard_core::{
        AssociationIntervalId, Checkout, CheckoutAvailability, CheckoutId, CheckoutPathId,
        CheckoutPathInterval, ConversationId, ConversationRef, DocumentId, EffectiveCheckout, Epic,
        EpicId, Feature, FeatureId, LaunchProfile, ManagedSessionRole, NativeSession,
        NativeSessionAssociation, Repository, RepositoryId, Resumability, Slug, Tool, WorkItem,
        WorkItemId, WorkItemKey, WorkItemStatus, WorkflowState, Workspace, WorkspaceId,
        WorkspaceSnapshot,
    };

    use super::{BoardControl, BoardState, LaunchOption, LaunchPickerState, plain, render};

    fn empty_snapshot() -> WorkspaceSnapshot {
        let planning_repository_id = RepositoryId::generate();
        WorkspaceSnapshot {
            workspace: Workspace {
                id: WorkspaceId::generate(),
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_repository_id: planning_repository_id,
            },
            repositories: vec![Repository {
                id: planning_repository_id,
                workspace_id: WorkspaceId::generate(),
                slug: Slug::new("planning-store").expect("repository slug"),
                title: "Planning store".to_owned(),
                git_common_directory: "C:/planning/.git".into(),
                default_branch: Some("main".to_owned()),
                remotes: Vec::new(),
                paths: Vec::new(),
            }],
            epics: Vec::new(),
            features: Vec::new(),
            work_items: Vec::new(),
            documents: Vec::new(),
            checkouts: Vec::new(),
            effective_checkouts: Vec::new(),
            sessions: Vec::new(),
            associations: Vec::new(),
        }
    }

    fn populated_snapshot() -> WorkspaceSnapshot {
        let mut snapshot = empty_snapshot();
        let workspace_id = snapshot.workspace.id;
        snapshot.repositories[0].workspace_id = workspace_id;
        let repository_id = RepositoryId::generate();
        snapshot.repositories.push(Repository {
            id: repository_id,
            workspace_id,
            slug: Slug::new("demo-code").expect("repository slug"),
            title: "Demo code".to_owned(),
            git_common_directory: "C:/code/.git".into(),
            default_branch: Some("main".to_owned()),
            remotes: Vec::new(),
            paths: Vec::new(),
        });
        let epic_id = EpicId::generate();
        snapshot.epics.push(Epic {
            id: epic_id,
            workspace_id,
            slug: Slug::new("launch").expect("Epic slug"),
            title: "Launch".to_owned(),
            document_id: DocumentId::generate(),
        });
        let feature_id = FeatureId::generate();
        snapshot.features.push(Feature {
            id: feature_id,
            epic_id,
            slug: Slug::new("availability").expect("Feature slug"),
            title: "Availability".to_owned(),
            document_id: Some(DocumentId::generate()),
            state: WorkflowState::ReconciliationRequired,
        });
        let work_item_id = WorkItemId::generate();
        snapshot.work_items.push(WorkItem {
            id: work_item_id,
            feature_id,
            key: WorkItemKey::new("launch/availability/api").expect("Work-item key"),
            slug: Slug::new("api").expect("Work-item slug"),
            title: "Availability API".to_owned(),
            status: WorkItemStatus::InProgress,
            document_id: DocumentId::generate(),
            repository_ids: vec![repository_id],
        });
        let checkout_id = CheckoutId::generate();
        snapshot.checkouts.push(Checkout {
            id: checkout_id,
            repository_id,
            git_worktree_identity: "availability-checkout".to_owned(),
            branch: Some("feature/availability".to_owned()),
            head: Some("0123456789abcdef".to_owned()),
            availability: CheckoutAvailability::Missing,
            replaces_checkout_id: None,
            paths: vec![CheckoutPathInterval {
                id: CheckoutPathId::generate(),
                checkout_id,
                path: "C:/worktrees/availability".into(),
                observed_from: OffsetDateTime::UNIX_EPOCH,
                observed_until: Some(OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1)),
            }],
        });
        snapshot.effective_checkouts.push(EffectiveCheckout {
            feature_id,
            work_item_id: Some(work_item_id),
            repository_id,
            checkout_id,
            inherited: true,
        });
        let session_id = ConversationId::generate();
        snapshot.sessions.push(NativeSession {
            id: session_id,
            native: ConversationRef::new(Tool::Codex, "thread-1").expect("conversation"),
            discovered_at: OffsetDateTime::UNIX_EPOCH,
        });
        snapshot.associations.push(NativeSessionAssociation {
            id: AssociationIntervalId::generate(),
            session_id,
            owner: workboard_core::HierarchyOwner::WorkItem(work_item_id),
            role: ManagedSessionRole::WorkItemExecution,
            associated_from: OffsetDateTime::UNIX_EPOCH,
            associated_until: None,
        });
        snapshot
    }

    fn render_text(snapshot: WorkspaceSnapshot, no_color: bool) -> String {
        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let state = BoardState::new(snapshot, no_color);
        terminal
            .draw(|frame| render(frame, &state))
            .expect("render board");
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn empty_terminal_fixture_is_explicit() {
        let rendered = render_text(empty_snapshot(), true);
        assert!(rendered.contains("No Epics"));
        assert!(rendered.contains("No matching Work items"));
        assert!(rendered.contains("Select a Work item"));
    }

    #[test]
    fn historical_missing_and_interrupted_fixture_is_visible() {
        let rendered = render_text(populated_snapshot(), true);
        assert!(rendered.contains("Launch"));
        assert!(rendered.contains("Availability API"));
        assert!(rendered.contains("missing"));
        assert!(rendered.contains("inherited"));
        assert!(rendered.contains("historical"));
        assert!(rendered.contains("Codex "));
        assert!(!rendered.contains("thread-1"));
        assert!(rendered.contains("Reconciliation required"));
    }

    #[test]
    fn keyboard_navigation_and_search_update_stable_selection() {
        let mut snapshot = populated_snapshot();
        let mut second = snapshot.work_items[0].clone();
        second.id = WorkItemId::generate();
        second.key = WorkItemKey::new("launch/availability/ui").expect("Work-item key");
        second.slug = Slug::new("ui").expect("Work-item slug");
        second.title = "Availability UI".to_owned();
        snapshot.work_items.push(second.clone());
        let mut state = BoardState::new(snapshot, true);
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            BoardControl::Continue
        );
        assert_eq!(
            state.selected_work_item().map(|item| item.id),
            Some(second.id)
        );
        state.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        for character in "api".chars() {
            state.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.visible_work_items().len(), 1);
        assert_eq!(
            state.selected_work_item().map(|item| item.title.as_str()),
            Some("Availability API")
        );
    }

    #[test]
    fn launch_picker_changes_profile_and_role_before_start() {
        let repository_id = RepositoryId::generate();
        let mut state = LaunchPickerState {
            title: "Start Work item".to_owned(),
            options: vec![LaunchOption {
                session_id: None,
                writer_reservation_key: None,
                repository_id,
                provider: Tool::Codex,
                profile: LaunchProfile::suggested(
                    Tool::Codex,
                    ManagedSessionRole::WorkItemExecution,
                ),
                role: ManagedSessionRole::WorkItemExecution,
                status: "new".to_owned(),
                last_activity: None,
                checkout: "C:/worktrees/materialize-on-start".into(),
                branch: None,
                resumability: Resumability::Unknown,
            }],
            selected: 0,
        };

        state.cycle_model();
        state.cycle_effort();
        state.cycle_role();
        let selection = state.selection().expect("launch selection");
        assert_eq!(selection.profile.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(
            selection.profile.effort.map(|value| value.as_str()),
            Some("xhigh")
        );
        assert_eq!(selection.role, ManagedSessionRole::Debugging);
        assert_eq!(selection.profile.role, ManagedSessionRole::Debugging);
    }

    #[test]
    fn large_catalogue_and_no_colour_noninteractive_fixtures_remain_usable() {
        let mut snapshot = populated_snapshot();
        let prototype = snapshot.work_items[0].clone();
        snapshot.work_items = (0..250)
            .map(|index| WorkItem {
                id: WorkItemId::generate(),
                key: WorkItemKey::new(format!("launch/availability/item-{index}"))
                    .expect("Work-item key"),
                slug: Slug::new(format!("item-{index}")).expect("Work-item slug"),
                title: format!("Work item {index:03}"),
                ..prototype.clone()
            })
            .collect();
        let rendered = render_text(snapshot.clone(), true);
        assert!(rendered.contains("Work item 000"));
        let plain = plain(&snapshot);
        assert!(plain.contains("Work items: 250"));
        assert!(!plain.contains('\u{1b}'));
    }
}
