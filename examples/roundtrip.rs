//! Opens each workbook given and saves it again: `roundtrip <in> <out>`...
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for pair in args.chunks(2) {
        let book = match umya_spreadsheet::reader::xlsx::read(&pair[0]) {
            Ok(b) => b,
            Err(e) => { eprintln!("{}: {e}", pair[0]); continue; }
        };
        if let Err(e) = umya_spreadsheet::writer::xlsx::write(&book, &pair[1]) {
            eprintln!("{}: {e}", pair[0]);
        }
    }
}
