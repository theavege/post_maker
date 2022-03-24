/*
    This file is part of Post Maker.
    Post Maker is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.
    Post Maker is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.
    You should have received a copy of the GNU General Public License
    along with Post Maker.  If not, see <https://www.gnu.org/licenses/>
*/

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, RwLock}, io::Read
};

use fltk::{button::Button, enums, prelude::*};
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageEncoder};
use serde::{Deserialize, Serialize};

use crate::result_ext::ResultExt;
use crate::globals;

/// helps cast tupels to f64
pub(crate) struct Coord(pub(crate) f64, pub(crate) f64);

impl From<(i32, i32)> for Coord {
    fn from((x, y): (i32, i32)) -> Self {
        Coord(x as f64, y as f64)
    }
}

impl From<(u32, u32)> for Coord {
    fn from((x, y): (u32, u32)) -> Self {
        Coord(x as f64, y as f64)
    }
}

impl From<(f32, f32)> for Coord {
    fn from((x, y): (f32, f32)) -> Self {
        Coord(x as f64, y as f64)
    }
}

impl Into<(f64, f64)> for Coord {
    fn into(self) -> (f64, f64) {
        (self.0, self.1)
    }
}

impl Into<(u32, u32)> for Coord {
    fn into(self) -> (u32, u32) {
        (self.0 as u32, self.1 as u32)
    }
}

impl Into<(i32, i32)> for Coord {
    fn into(self) -> (i32, i32) {
        (self.0 as i32, self.1 as i32)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ImageInfo {
    pub(crate) path: PathBuf,
    pub(crate) image_type: ImageType
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) enum ImageType {
    Jpeg,
    Png,
    Webp,
    None
}

impl ImageType {
    pub(crate) fn from_mime(v: &str) -> Self {
        match v {
            "image/jpeg" | "image/jpg" =>  Self::Jpeg,
            "image/png" =>  Self::Png,
            "image/webp" =>  Self::Webp,
            _ => Self::None
        }
    }

    pub(crate) fn as_extension(&self) -> String {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::None => "none"
        }.to_owned()
    }
}

/// Contains Image and its buffer(edited image)
#[derive(Debug, Clone)]
pub(crate) struct ImageContainer {
    pub(crate) image: DynamicImage,  //plain
    pub(crate) buffer: DynamicImage, //buffer to show
    pub(crate) properties: Arc<RwLock<ImageProperties>>,
}

impl ImageContainer {
    pub(crate) fn new(image_info: &ImageInfo, properties: Arc<RwLock<ImageProperties>>) -> Self {
        let img = load_image(&image_info);
        let (width, height): (f64, f64) = Coord::from(img.dimensions()).into();

        let config = globals::CONFIG.read().unwrap();
        let mut prop = properties.write().unwrap();
        prop.image_info = Some(image_info.to_owned());
        prop.original_dimension = (width, height);
        prop.quote_position = height * config.quote_position_ratio;
        prop.subquote_position = height * config.subquote_position_ratio;
        prop.subquote2_position = height * config.subquote2_position_ratio;
        prop.tag_position = height * config.tag_position_ratio;
        prop.tag2_position = height * config.tag2_position_ratio;

        Self {
            image: img.clone(),
            buffer: img,
            properties: Arc::clone(&properties),
        }
    }

    /// Resize image
    pub(crate) fn apply_resize(&mut self) {
        let mut prop = self.properties.write().unwrap();
        let (width, height) = prop.dimension;
        let (s_width, s_height) = ((width * 500.0) / height, 500.0);

        self.image = self.image.thumbnail_exact(s_width as u32, s_height as u32);

        self.buffer = self.image.clone();
        prop.dimension = (s_width, s_height);
    }

    /// Crop Image
    pub(crate) fn apply_crop(&mut self) {
        let mut prop = self.properties.write().unwrap();
        let (original_width, original_height) = prop.original_dimension;
        let (origina_crop_width, origina_crop_height) =
            croped_ratio(original_width, original_height);
        prop.crop_position = Some((
            (original_width - origina_crop_width) / 2.0,
            (original_height - origina_crop_height) / 2.0,
        ));

        let (s_width, s_height): (f64, f64) = Coord::from(self.image.dimensions()).into();
        let (c_width, c_height) = croped_ratio(s_width, s_height);
        let (cx, cy) = ((s_width - c_width) / 2.0, (s_height - c_height) / 2.0);

        prop.dimension = (c_width, c_height);

        self.image = self
            .image
            .crop(cx as u32, cy as u32, c_width as u32, c_height as u32);
        self.buffer = self.image.clone();
    }

    pub(crate) fn apply_crop_position(&mut self, original_x: f64, original_y: f64) {
        let mut prop = self.properties.write().unwrap();
        let (original_width, original_height) = prop.original_dimension;
        prop.crop_position = Some((original_x, original_y));

        let (s_width, s_height): (f64, f64) = Coord::from(self.image.dimensions()).into();
        let (c_width, c_height) = croped_ratio(s_width, s_height);
        let (cx, cy) = (
            (original_x * s_width) / original_width,
            (original_y * s_height) / original_height,
        );

        prop.dimension = (c_width, c_height);

        self.image = self
            .image
            .crop(cx as u32, cy as u32, c_width as u32, c_height as u32);
        self.buffer = self.image.clone();
    }

    /// Redraw: Copy image from main image to buffer and draw text and all on it
    pub(crate) fn redraw_to_buffer(&mut self) {
        let prop = self.properties.read().unwrap();
        let mut tmp = self.image.clone();

        draw_layer_and_text(
            &mut tmp,
            &prop.translucent_layer_color,
            &prop.quote,
            &prop.subquote,
            &prop.subquote2,
            prop.quote_position,
            prop.subquote_position,
            prop.subquote2_position,
            &prop.tag,
            &prop.tag2,
            prop.tag_position,
            prop.tag2_position,
            prop.original_dimension.1,
        );

        self.buffer = tmp;
    }

    /// Save image and properities
    pub(crate) fn save(&self) {
        let prop = self.properties.read().unwrap();

        let (path_original, mut original_image) = match &prop.image_info {
            Some(p) => (Path::new(&p.path), load_image(p)),
            None => return,
        };
        let path_properties = path_original.with_extension("prop");
        let config = globals::CONFIG.read().unwrap();
        let export_format = &config.image_format;
        let export = path_original.parent().unwrap().join("export").join(
            path_original
                .with_extension(export_format.as_extension())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        );

        let mut prop = prop.clone();
        prop.image_info = None;
        fs::write(&path_properties, serde_json::to_string(&ImagePropertiesFile::from(&prop)).unwrap()).warn_log("Failed to save properties!");

        let (width, height): (f64, f64) = Coord::from(original_image.dimensions()).into();
        let (crop_x, crop_y) = prop.crop_position.unwrap();
        let (crop_width, crop_height) = croped_ratio(width, height);
        let mut img = original_image.crop(
            crop_x as u32,
            crop_y as u32,
            crop_width as u32,
            crop_height as u32,
        );

        draw_layer_and_text(
            &mut img,
            &prop.translucent_layer_color,
            &prop.quote,
            &prop.subquote,
            &prop.subquote2,
            prop.quote_position,
            prop.subquote_position,
            prop.subquote2_position,
            &prop.tag,
            &prop.tag2,
            prop.tag_position,
            prop.tag2_position,
            prop.original_dimension.1,
        );

        let mut output = match File::create(&export) {
            Ok(a) => a,
            Err(e) => {
                Result::<(), _>::Err(e).warn_log("Failed to write to disk!");
                return;
            }
        };

        match export_format {
            ImageType::Png => {
                let encoder = image::codecs::png::PngEncoder::new_with_quality(
                    &mut output,
                    image::codecs::png::CompressionType::Best,
                    image::codecs::png::FilterType::Sub
                );

                let (w, h) = img.dimensions();
                encoder.write_image(&img.into_rgba8(), w, h, image::ColorType::Rgba8).warn_log("Failed to export Image!");
            }
            ImageType::Jpeg => {
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 100);
                encoder.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(300));

                encoder.encode_image(&img).warn_log("Failed to export Image!");
            }
            _ => (),
        }
    }

    pub(crate) fn clone_img(&self) -> Option<ImageInfo> {
        let prop = self.properties.read().unwrap();

        match &prop.image_info {
            Some(image_info) => {
                let name = image_info.path.file_stem().unwrap().to_string_lossy();
                let ext = image_info.path.extension().unwrap().to_string_lossy();
                let mut i = 1;
                let mut new_path = image_info.path.clone();
                while new_path.exists() {
                    let new_file = format!("{}{}.{}", name, "-copy".repeat(i), ext);
                    new_path = image_info.path.with_file_name(&new_file);
                    i += 1;
                }

                let path_properties = image_info.path.with_extension("prop");
                let path_properties_new = new_path.with_extension("prop");

                if image_info.path.exists() {
                    fs::copy(&image_info.path, &new_path).warn_log("Failed to clone image!");
                }

                if path_properties.exists() {
                    fs::copy(path_properties, &path_properties_new).warn_log("Failed to clone image properties!");
                }
                Some(ImageInfo {
                    path: new_path,
                    image_type: image_info.image_type.clone()
                })
            }
            None => None,
        }
    }

    pub(crate) fn delete(&self) {
        let prop = self.properties.read().unwrap();
        let config = globals::CONFIG.read().unwrap();
        let export_format = config.image_format.as_extension();

        let path_original = match &prop.image_info {
            Some(p) => Path::new(&p.path),
            None => return,
        };
        let path_properties = path_original.with_extension("prop");
        let export = path_original.parent().unwrap().join("export").join(
            path_original
                .with_extension(export_format)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        );

        if path_original.exists() {
            fs::remove_file(path_original).warn_log("Failed to delete image!");
        }

        if path_properties.exists() {
            fs::remove_file(path_properties).warn_log("Failed to delete image properties!");
        }

        if export.exists() {
            fs::remove_file(export).warn_log("Failed to delete exported image!");
        }
    }
}

/// Structure of Properties file of image to save and read
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ImagePropertiesFile {
    pub(crate) crop_position: Option<(f64, f64)>,
    pub(crate) quote: Option<String>,
    pub(crate) subquote: Option<String>,
    pub(crate) subquote2: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) tag2: Option<String>,
    pub(crate) quote_position: Option<f64>, // as per original
    pub(crate) subquote_position: Option<f64>, // as per original
    pub(crate) subquote2_position: Option<f64>, // as per original
    pub(crate) tag_position: Option<f64>,   // as per original
    pub(crate) tag2_position: Option<f64>,  // as per original
    pub(crate) translucent_layer_color: Option<[u8; 4]>,
}

impl Default for ImagePropertiesFile {
    fn default() -> Self {
        Self {
            crop_position: None,
            quote: None,
            subquote: None,
            subquote2: None,
            tag: None,
            tag2: None,
            quote_position: None,
            subquote_position: None,
            subquote2_position: None,
            tag_position: None,
            tag2_position: None,
            translucent_layer_color: None,
        }
    }
}

impl From<&ImageProperties> for ImagePropertiesFile {
    fn from(props: &ImageProperties) -> Self {
        Self {
            crop_position: props.crop_position,
            quote: Some(props.quote.clone()),
            subquote: Some(props.subquote.clone()),
            subquote2: Some(props.subquote2.clone()),
            tag: Some(props.tag.clone()),
            tag2: Some(props.tag2.clone()),
            quote_position: Some(props.quote_position),
            subquote_position: Some(props.subquote_position),
            subquote2_position: Some(props.subquote2_position),
            tag_position: Some(props.tag_position),
            tag2_position: Some(props.tag2_position),
            translucent_layer_color: Some(props.translucent_layer_color),
        }
    }
}

/// Properties of loaded image
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ImageProperties {
    pub(crate) image_info: Option<ImageInfo>,
    pub(crate) dimension: (f64, f64),
    pub(crate) original_dimension: (f64, f64),
    pub(crate) crop_position: Option<(f64, f64)>,
    pub(crate) quote: String,
    pub(crate) subquote: String,
    pub(crate) subquote2: String,
    pub(crate) tag: String,
    pub(crate) tag2: String,
    pub(crate) quote_position: f64,     // as per original
    pub(crate) subquote_position: f64,  // as per original
    pub(crate) subquote2_position: f64, // as per original
    pub(crate) tag_position: f64,       // as per original
    pub(crate) tag2_position: f64,      // as per original
    pub(crate) translucent_layer_color: [u8; 4],
    pub(crate) is_saved: bool,
}

impl Default for ImageProperties {
    fn default() -> Self {
        Self {
            image_info: None,
            dimension: (0.0, 0.0),
            original_dimension: (0.0, 0.0),
            crop_position: None,
            quote: "".to_owned(),
            subquote: "".to_owned(),
            subquote2: "".to_owned(),
            tag: "".to_owned(),
            tag2: "".to_owned(),
            quote_position: 0.0,
            subquote_position: 0.0,
            subquote2_position: 0.0,
            tag_position: 0.0,
            tag2_position: 0.0,
            translucent_layer_color: [0; 4],
            is_saved: true,
        }
    }
}

impl ImageProperties {
    pub(crate) fn merge(
        &mut self,
        props: ImagePropertiesFile,
        tag_default: &str,
        tag2_default: &str,
    ) {
        self.crop_position = props.crop_position;
        self.quote = props.quote.unwrap_or("".to_owned());
        self.subquote = props.subquote.unwrap_or("".to_owned());
        self.subquote2 = props.subquote2.unwrap_or("".to_owned());
        self.tag = props.tag.unwrap_or(tag_default.to_owned());
        self.tag2 = props.tag2.unwrap_or(tag2_default.to_owned());
        self.quote_position = props.quote_position.unwrap_or(self.quote_position);
        self.subquote_position = props.subquote_position.unwrap_or(self.subquote_position);
        self.subquote2_position = props.subquote2_position.unwrap_or(self.subquote2_position);
        self.tag_position = props.tag_position.unwrap_or(self.tag_position);
        self.tag2_position = props.tag2_position.unwrap_or(self.tag2_position);
        self.translucent_layer_color = props
            .translucent_layer_color
            .unwrap_or(globals::CONFIG.read().unwrap().color_layer);
    }
}

/// Load image as Dynamic Image
fn load_image(image_info: &ImageInfo) -> DynamicImage {
    let img = match image_info.image_type {
        ImageType::Webp => {
            let mut f = File::open(&image_info.path).expect_log("Failed to open image!");
            let mut buf = vec![];
            f.read_to_end(&mut buf).unwrap();
            let a = webp::Decoder::new(&buf).decode().unwrap();
            a.to_image()
        }
        ImageType::Jpeg => {
            let dec = image::codecs::jpeg::JpegDecoder::new(File::open(&image_info.path).expect_log("Failed to open image!")).expect_log("Failed to decode image!");
            DynamicImage::from_decoder(dec).expect_log("Failed to open image!")
        }
        ImageType::Png => {
            let dec = image::codecs::png::PngDecoder::new(File::open(&image_info.path).expect_log("Failed to open image!")).expect_log("Failed to decode image!");
            DynamicImage::from_decoder(dec).expect_log("Failed to open image!")
        }
        ImageType::None => {
            Result::<(), _>::Err("Failed to open image!").expect_log("");
            std::process::exit(1);
        }
    };

    DynamicImage::ImageRgb8(img.into_rgb8())
}

/// Draw text and stuffs on image
fn draw_layer_and_text(
    tmp: &mut DynamicImage,
    rgba: &[u8; 4],
    quote: &str,
    subquote: &str,
    subquote2: &str,
    quote_position: f64,
    subquote_position: f64,
    subquote2_position: f64,
    tag: &str,
    tag2: &str,
    tag_position: f64,
    tag2_position: f64,
    original_height: f64,
) {
    let (width, height): (f64, f64) = Coord::from(tmp.dimensions()).into();
    let layer =
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(width as u32, height as u32, |_, _| {
            image::Rgba(rgba.to_owned())
        }));
    image::imageops::overlay(tmp, &layer, 0, 0);

    let size = quote_from_height(height);
    draw_multiline_mid_string(
        tmp,
        &globals::FONT_QUOTE,
        size,
        quote_position,
        original_height,
        quote,
    );
    let size = subquote_from_height(height);
    draw_multiline_mid_string(
        tmp,
        &globals::FONT_SUBQUOTE,
        size,
        subquote_position,
        original_height,
        subquote,
    );
    let size = subquote2_from_height(height);
    draw_multiline_mid_string(
        tmp,
        &globals::FONT_SUBQUOTE2,
        size,
        subquote2_position,
        original_height,
        subquote2,
    );

    let size = tag2_from_height(height);
    draw_multiline_mid_string(
        tmp,
        &globals::FONT_TAG2,
        size,
        tag2_position,
        original_height,
        tag2,
    );

    let size = tag_from_height(height);
    for (index, line) in tag.lines().enumerate() {
        let (text_width, text_height) = measure_line(
            &globals::FONT_TAG,
            line,
            rusttype::Scale::uniform(size as f32),
        );

        imageproc::drawing::draw_text_mut(
            tmp,
            image::Rgba([255, 255, 255, 255]),
            (width * 0.99 - text_width) as i32,
            ((tag_position * height) / original_height + index as f64 * (text_height * 1.2)) as i32,
            rusttype::Scale::uniform(size as f32),
            &globals::FONT_TAG,
            line,
        );
    }
}

/// Draw multiline string on image
pub(crate) fn draw_multiline_mid_string(
    tmp: &mut DynamicImage,
    font: &rusttype::Font,
    size: f64,
    position: f64,
    original_height: f64,
    text: &str,
) {
    let (width, height): (f64, f64) = Coord::from(tmp.dimensions()).into();
    for (index, line) in text.lines().enumerate() {
        let (text_width, text_height) =
            measure_line(font, line, rusttype::Scale::uniform(size as f32));

        imageproc::drawing::draw_text_mut(
            tmp,
            image::Rgba([255, 255, 255, 255]),
            ((width - text_width) / 2.0) as i32,
            ((position * height) / original_height + index as f64 * (text_height * 1.15)) as i32,
            rusttype::Scale::uniform(size as f32),
            font,
            line,
        );
    }
}

/// Get size of text to draw on image
pub(crate) fn measure_line(
    font: &rusttype::Font,
    text: &str,
    scale: rusttype::Scale,
) -> (f64, f64) {
    let width = font
        .layout(text, scale, rusttype::point(0.0, 0.0))
        .map(|g| g.position().x + g.unpositioned().h_metrics().advance_width)
        .last()
        .unwrap_or(0.0);

    let v_metrics = font.v_metrics(scale);
    let height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;

    Coord::from((width, height)).into()
}

/// small hack because 0,0,0 rgb, because can't be set on fltk theme
pub(crate) fn set_color_btn_rgba(rgba: [u8; 4], btn: &mut Button) {
    let [mut r, g, b, _] = rgba;
    if r == 0 && g == 0 && b == 0 {
        r = 1;
    }
    btn.set_color(enums::Color::from_rgb(r, g, b));
}

/// Get required size to crop image as per image ratio
pub(crate) fn croped_ratio(width: f64, height: f64) -> (f64, f64) {
    if width > width_from_height(height) {
        (width_from_height(height), height)
    } else {
        (width, height_from_width(width))
    }
}

/// Get required witdh to crop image from height as per image ratio
pub(crate) fn width_from_height(height: f64) -> f64 {
    let (w, h) = globals::CONFIG.read().unwrap().image_ratio;
    (w * height) / h
}

/// Get required height to crop image from width as per image ratio
pub(crate) fn height_from_width(width: f64) -> f64 {
    let (w, h) = globals::CONFIG.read().unwrap().image_ratio;
    (h * width) / w
}

/// Get required quote size for crop image from height as per image ratio
pub(crate) fn quote_from_height(height: f64) -> f64 {
    (height * globals::CONFIG.read().unwrap().quote_font_ratio) / 5000.0
}

/// Get required subquote size for crop image from height as per image ratio
pub(crate) fn subquote_from_height(height: f64) -> f64 {
    (height * globals::CONFIG.read().unwrap().subquote_font_ratio) / 5000.0
}

/// Get required subquote2 size for crop image from height as per image ratio
pub(crate) fn subquote2_from_height(height: f64) -> f64 {
    (height * globals::CONFIG.read().unwrap().subquote2_font_ratio) / 5000.0
}

/// Get required tag size for crop image from height as per image ratio
pub(crate) fn tag_from_height(height: f64) -> f64 {
    (height * globals::CONFIG.read().unwrap().tag_font_ratio) / 5000.0
}

/// Get required tag2 size for crop image from height as per image ratio
pub(crate) fn tag2_from_height(height: f64) -> f64 {
    (height * globals::CONFIG.read().unwrap().tag2_font_ratio) / 5000.0
}
