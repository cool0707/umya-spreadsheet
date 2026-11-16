//! Threaded comments must keep their authors, their replies, whether they
//! are resolved and their mentions when a workbook is saved, and the note
//! Excel writes in their place its `xr:uid`.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    Comment,
    Workbook,
    reader,
    structs::office2019::{
        person::Person,
        threaded_comment::{
            Mention,
            ThreadedComment,
        },
    },
    writer,
};

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn comment(cell: &str, id: &str, person: &str, text: &str) -> ThreadedComment {
    let mut c = ThreadedComment::default();
    c.coordinate_mut().set_coordinate(cell);
    c.set_id(id).set_person_id(person).set_text(text).set_d_t("2026-11-15T23:09:53.90");
    c
}

#[test]
fn threaded_comments_round_trip() {
    let mut book = umya_spreadsheet::new_file();
    book.persons_mut().push(Person {
        display_name: "山田 & 太郎".into(),
        id:           "{P1}".into(),
        user_id:      "山田 & 太郎".into(),
        provider_id:  "None".into(),
    });
    let ws = book.sheet_mut(0).unwrap();
    let mut first = comment("B2", "{C1}", "{P1}", "最初 <a>");
    first.set_done(true);
    ws.add_threaded_comments(first);
    let mut reply = comment("B2", "{C2}", "{P1}", "@山田 & 太郎 返信");
    reply.set_parent_id("{C1}");
    reply.mentions_mut().push(Mention { person_id: "{P1}".into(), id: "{M1}".into(), start_index: 0, length: 8 });
    ws.add_threaded_comments(reply);
    let mut note = Comment::default();
    note.new_comment("B2");
    note.set_author("tc={C1}").set_uid("{C1}").set_text_string("[Threaded comment]");
    ws.add_comments(note);

    let bytes = write(&book);
    let comments = part(&bytes, "xl/comments1.xml");
    assert!(comments.contains(r#"xr:uid="{C1}""#), "{comments}");
    assert!(comments.contains("mc:Ignorable=\"xr\""), "{comments}");
    let types = part(&bytes, "[Content_Types].xml");
    assert!(types.contains(r#"PartName="/xl/threadedComments/threadedComment1.xml" ContentType="application/vnd.ms-excel.threadedcomments+xml""#), "{types}");
    assert!(types.contains(r#"PartName="/xl/persons/person.xml" ContentType="application/vnd.ms-excel.person+xml""#), "{types}");

    let back = reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap();
    assert_eq!(back.persons(), book.persons());
    let ws = back.sheet(0).unwrap();
    let list = ws.threaded_comments();
    assert_eq!(list.len(), 2);
    assert_eq!((list[0].id(), list[0].person_id(), list[0].parent_id(), list[0].done()), ("{C1}", "{P1}", "", true));
    assert_eq!(list[0].text(), "最初 <a>");
    assert_eq!((list[1].parent_id(), list[1].done()), ("{C1}", false));
    assert_eq!(list[1].mentions(), &[Mention { person_id: "{P1}".into(), id: "{M1}".into(), start_index: 0, length: 8 }]);
    assert_eq!(ws.comments()[0].uid(), "{C1}");
}

#[test]
fn inserted_rows_move_threaded_comments() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    ws.add_threaded_comments(comment("B2", "{C1}", "{P1}", "a"));
    ws.add_threaded_comments(comment("B5", "{C2}", "{P1}", "b"));
    ws.insert_new_row(2, 3);
    ws.remove_row(8, 1);
    let refs: Vec<String> = ws.threaded_comments().iter().map(|c| c.coordinate().to_string()).collect();
    assert_eq!(refs, ["B5"]);
}
