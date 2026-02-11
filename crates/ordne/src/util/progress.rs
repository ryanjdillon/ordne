use indicatif::{ProgressBar, ProgressStyle};

pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})")
            .expect("Failed to create progress template")
            .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb
}
