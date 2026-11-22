//! Reading the drawings of a workbook from its parts, for programs that draw
//! them (as HTML/SVG, in print layouts and previews) rather than keep them:
//! what is shared by xljs (its printing of sheets) and RAS (its view of a
//! workbook). The parts are read as [`crate::xml_tree`] nodes from the file
//! itself, as the structs of this crate leave out much of what drawings
//! hold.
//!
//! - [`geometry`]: the preset shapes, the text warps of `WordArt` and freeforms
//!   as SVG paths.
//! - [`color`]: the colours of `DrawingML` (theme, modifiers, fills, shape
//!   styles).
//! - [`anchor`]: where an object is on its sheet (its anchor) and its
//!   transform.
//! - [`chart`]: the automatic colours of a chart's series and the data of a
//!   series as the chart part holds it.
//! - [`package`]: the relationships between the parts.

pub mod anchor;
pub mod chart;
pub mod color;
pub mod geometry;
pub mod package;

/// EMU in a point.
pub const EMU_PT: f64 = 12700.0;
/// EMU in a pixel (at 96 dpi).
pub const EMU_PX: f64 = 9525.0;
