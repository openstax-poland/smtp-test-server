// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

use std::{io::{self, Write, BufWriter}, path::Path, env, fs::{File, self}};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=page/dist");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("page_data.rs");
    let mut out = BufWriter::new(File::create(dest_path)?);

    writeln!(out, "static PAGE_DATA: &[File] = &[")?;

    for entry in fs::read_dir("page/dist")? {
        let entry = entry?;

        if !entry.metadata()?.is_file() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path().canonicalize()?;
        writeln!(out, "    File {{ name: {name:?}, data: include_bytes!({path:?}) }},")?;
    }

    writeln!(out, "];")?;
    out.flush()?;

    Ok(())
}
