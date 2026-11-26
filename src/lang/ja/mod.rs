//! The Japanese, one part for each part of the program, keyed by the German
//! the source says. See [`super`].

mod artist;
mod errors;
mod folder;
mod library;
mod menu;
mod shell;
mod window;

pub const ALL: &[&[(&str, &str)]] = &[
    library::PART,
    window::PART,
    menu::PART,
    folder::PART,
    shell::PART,
    errors::PART,
    artist::PART,
];
