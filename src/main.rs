use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::PathBuf;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = match args.get(1) {
        Some(dir) => dir,
        None => {
            eprintln!("Directory is required");
            process::exit(1);
        }
    };

    let paths: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(paths) => paths.map(|f| f.unwrap().path()).collect(),
        Err(_) => {
            eprintln!("Could not open directory {}", dir);
            process::exit(2);
        }
    };

    rexiv2::set_log_level(rexiv2::LogLevel::ERROR);

    let progress = ProgressBar::new(paths.len() as u64);
    progress.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (ETA: {eta})",
    ));

    let errors: Vec<String> = paths
        .par_iter()
        .progress_with(progress)
        .map(|p| process_file(&p))
        .filter_map(|r| r.err())
        .collect();

    for error in &errors {
        eprintln!("Failed to process {}", error.to_string());
    }

    println!("Processed {} files", paths.len() - errors.len());
    println!("Failed to process {} files", errors.len());
}

fn process_file(path: &PathBuf) -> Result<(), String> {
    let path_str = path.to_str().unwrap();

    let meta = rexiv2::Metadata::new_from_path(&path)
        .map_err(|_| format!("{}: Could not read metadata", path_str))?;

    let date = meta
        .get_tag_string("Exif.GPSInfo.GPSDateStamp")
        .map_err(|_| format!("{}: Image is missing GPSDateStamp value", path_str))?;

    let time = meta
        .get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")
        .map_err(|_| format!("{}: Image is missing GPSTimeStamp value", path_str))?;

    // Source for capacity: YY:MM:DD HH:MM:SS
    let mut date_time = String::with_capacity(19);
    date_time.push_str(&date);
    date_time.push(' ');
    date_time.push_str(&time);

    meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)
        .map_err(|_| format!("{}: Failed to save DateTimeOriginal value", path_str))?;

    meta.save_to_file(&path)
        .map_err(|_| format!("{}: Failed to save file", path_str))?;

    Ok(())
}
