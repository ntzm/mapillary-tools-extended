use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rexiv2;
use std::fs;
use std::path::PathBuf;

fn main() {
    let paths: Vec<PathBuf> = fs::read_dir("data")
        .unwrap()
        .map(|f| f.unwrap().path())
        .collect();

    rexiv2::set_log_level(rexiv2::LogLevel::ERROR);

    let progress = ProgressBar::new(paths.len() as u64);
    progress.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (ETA: {eta})",
    ));

    let errors: Vec<rexiv2::Rexiv2Error> = paths
        .par_iter()
        .progress_with(progress)
        .map(|path| {
            let meta = rexiv2::Metadata::new_from_path(&path)?;
            let date = meta.get_tag_string("Exif.GPSInfo.GPSDateStamp")?;
            let time = meta.get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")?;
            // Source for capacity: YY:MM:DD HH:MM:SS
            let mut date_time = String::with_capacity(19);
            date_time.push_str(&date);
            date_time.push_str(" ");
            date_time.push_str(&time);
            meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)?;
            meta.save_to_file(&path)?;
            Ok(())
        })
        .filter_map(|r| r.err())
        .collect();

    for error in errors {
        println!("{}", error.to_string());
    }
}
