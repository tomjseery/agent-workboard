use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionCandidate {
    pub id: String,
    pub key: Option<String>,
    pub label: String,
    pub metadata: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCandidate {
    pub candidate: SelectionCandidate,
    pub score: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionResult {
    Empty,
    Selected(SelectionCandidate),
    Picker(Vec<RankedCandidate>),
}

pub fn resolve(
    query: Option<&str>,
    candidates: impl IntoIterator<Item = SelectionCandidate>,
) -> SelectionResult {
    let candidates: Vec<_> = candidates.into_iter().collect();
    if candidates.is_empty() {
        return SelectionResult::Empty;
    }
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return SelectionResult::Picker(rank_all(candidates));
    };
    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(query))
    {
        return SelectionResult::Selected(candidate.clone());
    }
    let exact: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .key
                .as_deref()
                .is_some_and(|key| key.eq_ignore_ascii_case(query))
                || candidate.label.eq_ignore_ascii_case(query)
        })
        .cloned()
        .collect();
    if let [candidate] = exact.as_slice() {
        return SelectionResult::Selected(candidate.clone());
    }
    if exact.len() > 1 {
        return SelectionResult::Picker(rank_all(exact));
    }
    let mut matches: Vec<_> = candidates
        .into_iter()
        .filter_map(|candidate| {
            candidate_score(query, &candidate).map(|score| RankedCandidate { candidate, score })
        })
        .collect();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.candidate.label.cmp(&right.candidate.label))
            .then_with(|| left.candidate.id.cmp(&right.candidate.id))
    });
    match matches.as_slice() {
        [] => SelectionResult::Empty,
        [candidate] => SelectionResult::Selected(candidate.candidate.clone()),
        _ => SelectionResult::Picker(matches),
    }
}

fn rank_all(candidates: Vec<SelectionCandidate>) -> Vec<RankedCandidate> {
    let mut candidates: Vec<_> = candidates
        .into_iter()
        .map(|candidate| RankedCandidate {
            candidate,
            score: 0,
        })
        .collect();
    candidates.sort_by(|left, right| {
        left.candidate
            .label
            .cmp(&right.candidate.label)
            .then_with(|| left.candidate.id.cmp(&right.candidate.id))
    });
    candidates
}

fn candidate_score(query: &str, candidate: &SelectionCandidate) -> Option<u32> {
    [
        candidate.label.as_str(),
        candidate.key.as_deref().unwrap_or_default(),
        candidate.metadata.as_str(),
    ]
    .into_iter()
    .filter_map(|value| fuzzy_score(query, value))
    .max()
}

fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    let query = query.to_ascii_lowercase();
    let candidate = candidate.to_ascii_lowercase();
    if candidate.starts_with(&query) {
        return Some(10_000_u32.saturating_sub(candidate.len() as u32));
    }
    if let Some(index) = candidate.find(&query) {
        return Some(8_000_u32.saturating_sub(index as u32));
    }
    let mut query_characters = query.chars();
    let mut wanted = query_characters.next()?;
    let mut gaps = 0_u32;
    for character in candidate.chars() {
        if character == wanted {
            if let Some(next) = query_characters.next() {
                wanted = next;
            } else {
                return Some(5_000_u32.saturating_sub(gaps));
            }
        } else {
            gaps = gaps.saturating_add(1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{SelectionCandidate, SelectionResult, resolve};

    fn candidate(id: usize, label: &str) -> SelectionCandidate {
        SelectionCandidate {
            id: format!("id-{id}"),
            key: Some(format!("launch/item-{id}")),
            label: label.to_owned(),
            metadata: "codex ready".to_owned(),
        }
    }

    #[test]
    fn handles_empty_single_and_exact_id_resolution() {
        assert_eq!(resolve(None, Vec::new()), SelectionResult::Empty);
        let only = candidate(1, "Availability API");
        assert_eq!(
            resolve(Some("availability"), [only.clone()]),
            SelectionResult::Selected(only.clone())
        );
        assert_eq!(
            resolve(Some("id-1"), [only.clone(), candidate(2, "Other")]),
            SelectionResult::Selected(only)
        );
    }

    #[test]
    fn ambiguous_queries_fall_back_to_a_ranked_picker() {
        let result = resolve(
            Some("availability"),
            [
                candidate(1, "Availability API"),
                candidate(2, "Availability UI"),
            ],
        );
        let SelectionResult::Picker(matches) = result else {
            panic!("ambiguous query should open the picker");
        };
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn large_catalogues_are_deterministic() {
        let candidates = (0..1_000)
            .map(|index| candidate(index, &format!("Work item {index:04}")))
            .collect::<Vec<_>>();
        let SelectionResult::Picker(matches) = resolve(Some("work item 09"), candidates) else {
            panic!("large query should remain ambiguous");
        };
        assert!(matches.len() >= 100);
        assert_eq!(matches[0].candidate.label, "Work item 0900");
        assert_eq!(matches[99].candidate.label, "Work item 0999");
    }
}
