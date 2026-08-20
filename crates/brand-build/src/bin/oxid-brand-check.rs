// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, path::PathBuf, process::ExitCode};

use oxid_brand_build::{check_brand_path, load_brand_pack, render_brand_css};

fn main() -> ExitCode {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [flag, pack] if flag == "--css" => {
            load_brand_pack(PathBuf::from(pack)).map(|brand| print!("{}", render_brand_css(&brand)))
        }
        [path] => check_brand_path(PathBuf::from(path)).map(|names| {
            println!("validated brand packs: {}", names.join(", "));
        }),
        _ => {
            eprintln!(
                "usage: oxid-brand-check <brands-root|brand-pack>\n       oxid-brand-check --css <brand-pack>"
            );
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("brand validation failed: {error}");
            ExitCode::FAILURE
        }
    }
}
