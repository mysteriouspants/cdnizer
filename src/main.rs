use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::fs::{create_dir, File, remove_dir_all, remove_file};
use std::path::{Component, Path};

use askama::Template;
use chrono::{DateTime, Utc};
use include_dir::{Dir, include_dir};
use serde::Serialize;
use size::Size;

static VENDOR_DIR_NAME: &str = "_vendor";
static VENDOR_DIR: Dir = include_dir!("vendor");

#[derive(Debug)]
struct Breadcrumb {
    name: String,
    path: String,
}

#[derive(Clone, Debug, Serialize)]
struct Entry {
    name: String,
    #[serde(skip)]
    path: String,
    #[serde(skip)]
    icon: String,
    date: DateTime<Utc>,
    size: String,
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexHtml {
    vendor_dir: String,
    breadcrumbs: Vec<Breadcrumb>,
    entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
struct IndexJson {
    entries: Vec<Entry>,
}


fn main() -> color_eyre::Result<()> {
    // 1. write out vendor dir
    remove_dir_all(VENDOR_DIR_NAME)?;
    create_dir(VENDOR_DIR_NAME)?;
    VENDOR_DIR.extract(VENDOR_DIR_NAME)?;

    // 2. walk the cwd tree and generate index.html and index.json
    //    files to make navigating the cdn easier
    generate_index(".")
}

impl Entry {
    fn new<P: AsRef<Path>>(path: P) -> color_eyre::Result<Self> {
        let path = path.as_ref();
        let metadata = path.metadata()?;
        let size = match path.is_dir() {
            true => "directory".to_string(),
            false => format!("{}", Size::from_bytes(metadata.len())),
        };
        let date = DateTime::from(metadata.modified()?);

        Ok(Self {
            name: path.file_name()
                .map(|os_str| os_str.to_string_lossy())
                .unwrap_or(Cow::Borrowed("")).to_string(),
            path: path.to_web_path(),
            icon: icon(&path).to_string(),
            date,
            size,
        })
    }
}

fn generate_index<P: AsRef<Path>>(path: P) -> color_eyre::Result<()> {
    eprintln!("Generating indicies for {:?}", path.as_ref());
    let mut directories = vec![];
    let mut files = vec![];

    for dir_entry in path.as_ref().read_dir()? {
        if let Ok(dir_entry) = dir_entry {
            let entry_path = dir_entry.path();

            if ignore(&entry_path) {
                continue;
            }

            let entry = Entry::new(entry_path.clone())?;

            if dir_entry.file_type()?.is_dir() {
                generate_index(&entry_path.as_path())?;

                directories.push(entry);
            } else {
                files.push(entry);
            }
        }
    }

    directories.sort_by(|a, b| a.name.cmp(&b.name));
    files.sort_by(|a, b| b.date.cmp(&a.date));

    let index_json = path.as_ref().join("index.json");
    let index_json = {
        if index_json.exists() {
            remove_file(&index_json)?;
        }

        File::create(index_json)?
    };

    serde_json::to_writer_pretty(index_json, &IndexJson {
        entries: files.clone()
    })?;

    let entries = directories.into_iter().chain(files.into_iter()).collect::<Vec<_>>();

    let index_html = path.as_ref().join("index.html");
    let mut index_html = {
        if index_html.exists() {
            remove_file(&index_html)?;
        }

        File::create(index_html)?
    };

    let breadcrumbs = path.as_ref().to_breadcrumbs();

    IndexHtml {
        vendor_dir: VENDOR_DIR_NAME.to_string(),
        breadcrumbs,
        entries,
    }.write_into(&mut index_html)?;

    Ok(())
}

fn ignore(path: &Path) -> bool {
    path.file_name().map(|os_str| os_str.to_string_lossy() == VENDOR_DIR_NAME).unwrap_or(false) ||
        path.file_name().map(|fname| (
            fname == "index.html" || fname == "index.json"
        )).unwrap_or(false)
}

fn icon(path: &Path) -> &'static str {
    if path.is_dir() {
        return "dir.png";
    }

    match path.extension().unwrap_or(OsStr::new("")).to_string_lossy().as_ref() {
        ".." => "back.png",
        "" | "^^BLANKICON^^" => "blank.gif",
        // archives
        "comp" => "comp.png",
        "zip" | "tar" | "tgz" | "rar" | "gz" | "bz2" => "compressed.gif",
        // office formats
        "doc" | "docx" => "doc.png",
        "xls" | "xlsx" => "xls.png",
        "ppt" | "pptx" => "ppt.png",
        "txt" | "text" | "html" | "htm" | "md" | "mdown" | "markdown" => "text.png",
        "pdf" => "pdf.png",
        // still media
        "jpg" | "jpeg" | "png" | "gif" | "tif" | "tiff" | "webp" => "image.png",
        "ps" => "ps.png",
        // audio media
        "mp3" | "wav" | "m4a" | "ogg" => "sound.png",
        // video media
        "wmv" | "avi" | "mp4" | "webm" => "movie-ms.gif",
        "mov" | "qt" => "mov.png",
        // nerd stuff
        "java" => "java.png",
        "js" => "js.png",
        "php" => "php.png",

        _ => "text.png"
    }
}

trait ToWebPath {
    fn to_web_path(&self) -> String;
}

impl ToWebPath for Path {
    fn to_web_path(&self) -> String {
        self.components()
            .map(|c| match c {
                Component::Normal(elem) => elem.to_os_string(),
                _ => OsString::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(OsStr::new("/"))
            .to_string_lossy()
            .to_string()
    }
}

trait ToBreadcrumbs {
    fn to_breadcrumbs(&self) -> Vec<Breadcrumb>;
}

impl ToBreadcrumbs for Path {
    fn to_breadcrumbs(&self) -> Vec<Breadcrumb> {
        let mut current_path = Some(self);
        let mut crumbs = Vec::with_capacity(self.components().count());

        while let Some(path) = current_path {
            if path == OsStr::new(".") {
                break;
            }

            crumbs.push(Breadcrumb {
                path: path.to_web_path(),
                name: path.file_name().unwrap_or(OsStr::new("")).to_string_lossy().to_string(),
            });
            current_path = path.parent();
        }

        crumbs.reverse();

        crumbs
    }
}
