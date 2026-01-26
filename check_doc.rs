// Check if-addrs structure
use std::process::Command;
let output = Command::new("cargo")
    .args(&["doc", "--package", "if-addrs", "--no-deps"])
    .output()
    .unwrap();
println!("{}", String::from_utf8_lossy(&output.stdout));
