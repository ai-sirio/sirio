fn main() {
    let path = std::env::args().nth(1).expect("output path");
    let text = serde_json::to_string_pretty(&sirio_host_protocol::wire_schema()).unwrap() + "\n";
    std::fs::write(path, text).unwrap();
}
