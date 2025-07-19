include!(concat!(env!("OUT_DIR"), "/opencv/calib3d.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/core.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/dnn.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/features2d.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/flann.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/gapi.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/highgui.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/imgcodecs.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/imgproc.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/ml.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/objdetect.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/photo.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/stitching.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/video.rs"));
include!(concat!(env!("OUT_DIR"), "/opencv/videoio.rs"));
pub mod types {
	include!(concat!(env!("OUT_DIR"), "/opencv/types.rs"));
}
#[doc(hidden)]
pub mod sys {
	include!(concat!(env!("OUT_DIR"), "/opencv/sys.rs"));
}
pub mod hub_prelude {
	pub use super::calib3d::prelude::*;
	pub use super::core::prelude::*;
	pub use super::dnn::prelude::*;
	pub use super::features2d::prelude::*;
	pub use super::flann::prelude::*;
	pub use super::gapi::prelude::*;
	pub use super::highgui::prelude::*;
	pub use super::imgcodecs::prelude::*;
	pub use super::imgproc::prelude::*;
	pub use super::ml::prelude::*;
	pub use super::objdetect::prelude::*;
	pub use super::photo::prelude::*;
	pub use super::stitching::prelude::*;
	pub use super::video::prelude::*;
	pub use super::videoio::prelude::*;
}

mod ffi_exports {
	use crate::mod_prelude_sys::*;
	#[no_mangle] unsafe extern "C" fn ocvrs_create_string_0_94_4(s: *const c_char) -> *mut String { unsafe { crate::templ::ocvrs_create_string(s) } }
	#[no_mangle] unsafe extern "C" fn ocvrs_create_byte_string_0_94_4(v: *const u8, len: size_t) -> *mut Vec<u8> { unsafe { crate::templ::ocvrs_create_byte_string(v, len) } }
}
