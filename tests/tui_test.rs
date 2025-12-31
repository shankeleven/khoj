use rexpect::session::PtySession;
use rexpect::spawn;
use std::error::Error;
use std::str;

fn clean_output(s: &str) -> String {
    let mut cleaned = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else if c == '\x1b' {
            in_escape = true;
        } else {
            cleaned.push(c);
        }
    }
    cleaned
}

#[test]
fn test_tui() -> Result<(), Box<dyn Error>> {
    let mut p: PtySession = spawn("./target/debug/khoj", Some(10000))?;
    let buffer = p.read_line()?;
    let cleaned_buffer = clean_output(&buffer);
    println!("Cleaned buffer: {}", cleaned_buffer);
    assert!(cleaned_buffer.contains("Search"));
    println!("Found 'Search'");
    p.send_line("test")?;
    println!("Sent 'test'");
    let buffer = p.read_line()?;
    let cleaned_buffer = clean_output(&buffer);
    println!("Cleaned buffer: {}", cleaned_buffer);
    assert!(cleaned_buffer.contains("Results"));
    println!("Found 'Results'");
    p.send_control('c')?;
    Ok(())
}
