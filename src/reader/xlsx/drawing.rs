use quick_xml::{
    Reader,
    events::Event,
};

use crate::{
    reader::driver::xml_read_loop,
    structs::{
        PartStore,
        Worksheet,
        drawing::spreadsheet::{
            WorksheetDrawing,
            ShapeAnchor,
            TakenAnchors,
            take_anchors,
        },
        raw::{
            RawFile,
            RawRelationships,
        },
    },
};

pub(crate) fn read(
    worksheet: &mut Worksheet,
    drawing_file: &RawFile,
    drawing_relationships: Option<&RawRelationships>,
    chart_parts: &PartStore,
) {
    // The anchors that hold charts are kept as they were read, with the
    // parts of the charts, and so are those of shapes; the rest is read into
    // the model.
    let (data, kept) =
        match take_anchors(drawing_file.file_data(), drawing_relationships, chart_parts) {
            Some((data, kept)) => (std::borrow::Cow::Owned(data), kept),
            None => (
                std::borrow::Cow::Borrowed(drawing_file.file_data()),
                TakenAnchors { charts: Vec::new(), shapes: Vec::new() },
            ),
        };
    let data = std::io::Cursor::new(data.as_ref());
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);

    xml_read_loop!(
        reader,
        Event::Start(ref e) => {
            if matches!(e.name().into_inner(), b"xdr:wsDr" | b"wsDr") {
                let mut obj = WorksheetDrawing::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    drawing_relationships,
                    worksheet.ole_objects_mut(),
                );
                worksheet.set_worksheet_drawing(obj);
            }
        },
        Event::Eof => break
    );
    // Charts of the kinds the model knows are read into it as well (and
    // written as they were read while they are not changed).
    for anchor in kept.charts {
        match anchor.into_model_chart(drawing_relationships) {
            Ok(chart) => {
                worksheet
                    .worksheet_drawing_mut()
                    .add_chart_collection(chart);
            }
            Err(anchor) => {
                worksheet
                    .worksheet_drawing_mut()
                    .kept_anchor_collection_mut()
                    .push(anchor);
            }
        }
    }
    // Shapes are read into the model as well, and written as they were read
    // while they are not changed.
    let drawing = worksheet.worksheet_drawing_mut();
    for anchor in kept.shapes {
        let model_images = anchor.model_images().any(|i| i.is_some());
        match anchor.into_model_shape(drawing_relationships) {
            Ok(ShapeAnchor::Two(mut anchor)) => {
                if anchor.is_image() {
                    let mut image = crate::structs::Image::default();
                    image.set_two_cell_anchor(anchor);
                    drawing.add_image(image);
                } else {
                    if model_images {
                        // Its pictures are the model's only in pictures.
                        anchor.forget_kept();
                    }
                    drawing.add_two_cell_anchor_collection(anchor);
                }
            }
            Ok(ShapeAnchor::One(mut anchor)) => {
                if anchor.is_image() {
                    let mut image = crate::structs::Image::default();
                    image.set_one_cell_anchor(anchor);
                    drawing.add_image(image);
                } else {
                    if model_images {
                        anchor.forget_kept();
                    }
                    drawing.add_one_cell_anchor_collection(anchor);
                }
            }
            // (One with pictures the model would write is dropped, as the
            // model drops it.)
            Err(anchor) if !model_images => drawing.kept_anchor_collection_mut().push(anchor),
            Err(_) => {}
        }
    }
}
