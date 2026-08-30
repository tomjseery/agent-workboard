#![forbid(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    workboard_desktop::run()
}
