use quick_xml::{
    Reader,
    events::Event,
};

use crate::{
    structs::{
        Worksheet,
        raw::{
            RawFile,
            RawRelationships,
        },
        vml::Shape,
    },
};

pub(crate) fn read(
    worksheet: &mut Worksheet,
    drawing_file: &RawFile,
    drawing_relationships: Option<&RawRelationships>,
) {
    let raw = drawing_file.file_data();
    let data = std::io::Cursor::new(raw);
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);

    let mut ole_index = 0;
    let mut comment_index = 0;
    let mut buf = Vec::new();
    // The namespaces of the root but those the writer declares on its own
    // (`xmlns:w10`...), for the shapes kept to declare those they use.
    let mut namespaces: Vec<(String, String)> = Vec::new();

    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(0);
        buf.clear();
        let e = match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => e.into_owned(),
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => continue,
        };
        match e.name().into_inner() {
            b"v:shape" => {
                let mut obj = Shape::default();
                obj.set_attributes(&mut reader, &e, drawing_relationships);
                let after = usize::try_from(reader.buffer_position()).unwrap_or(0);
                if obj.client_data().comment_column_target().is_some() {
                    // The frame of the comment as it was read.
                    if let Some(xml) = raw.get(before..after).and_then(|x| std::str::from_utf8(x).ok()) {
                        obj.set_kept(&crate::structs::declare_namespaces(xml.trim_start(), &namespaces));
                    }
                    if let Some(comment) = worksheet.comments_mut().get_mut(comment_index) {
                        comment.set_shape(obj);
                    }
                    comment_index += 1;
                } else {
                    if let Some(ole_obj) = worksheet
                        .ole_objects_mut()
                        .ole_object_mut()
                        .get_mut(ole_index)
                    {
                        ole_obj.set_shape(obj);
                    }
                    ole_index += 1;
                }
            }
            b"xml" => {
                namespaces = crate::structs::root_namespaces(&e)
                    .into_iter()
                    .filter(|(k, _)| !matches!(k.as_str(), "xmlns:v" | "xmlns:o" | "xmlns:x"))
                    .collect();
            }
            // The types of the shapes, for the frames of comments of other
            // shapes than Excel's text box.
            b"v:shapetype" => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, &e, false);
                worksheet.kept_relationships_mut().add_xml("v:shapetype", xml);
            }
            _ => {}
        }
    }
}

// fn set_style(comment:&mut Comment, style_string:&str) {
//    let styles: Vec<&str> = style_string.split(';').collect();
//    for style in &styles {
//        let params: Vec<&str> = style.split(':').collect();
//        if params.len() == 2 {
//            let key_string = params[0].replace(" ", "").replace("\r\n", "");
//            let key = key_string.as_str();
//            let value = params[1].replace(" ", "").replace("\r\n", "");
//            match key {
//                "margin-left" => comment.set_margin_left(value),
//                "margin-top" => comment.set_margin_top(value),
//                "width" => comment.set_width(value),
//                "height" => comment.set_height(value),
//                "visibility" => comment.set_visible(if value == "visible" {
// true } else { false }),                _ => {}
//            }
//        }
//    }
//}
