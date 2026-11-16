// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

/// The product's name as people see it: window titles, menus, the About box
/// and installers.
pub const PRODUCT_NAME: &str = "VLabs";

/// The name of the program file, without the `.exe` ending Windows adds.
///
/// Lower-case letters, digits and `-` only, so it is safe to type and safe in
/// file paths on every system.
pub const EXECUTABLE_NAME: &str = "vlabs";

/// The ending of project files, without the leading dot.
///
/// Lower-case letters and digits only.
pub const PROJECT_FILE_EXTENSION: &str = "teto";

/// The application identifier, in reverse-domain form.
///
/// Desktops use it to recognise the application: the Linux desktop entry and
/// window class, and the Windows taskbar identity. Changing it after a public
/// release makes existing installations look like a different application.
pub const APPLICATION_ID: &str = "io.github.ofujiiro.VLabs";

#[cfg(test)]
mod tests {
    use super::{APPLICATION_ID, EXECUTABLE_NAME, PRODUCT_NAME, PROJECT_FILE_EXTENSION};

    #[test]
    fn product_name_is_presentable() {
        assert!(!PRODUCT_NAME.is_empty(), "the product name is empty");
        assert_eq!(
            PRODUCT_NAME,
            PRODUCT_NAME.trim(),
            "the product name starts or ends with a space"
        );
    }

    #[test]
    fn executable_name_is_safe_in_file_paths() {
        assert!(
            !EXECUTABLE_NAME.is_empty(),
            "the program file name is empty"
        );
        assert!(
            EXECUTABLE_NAME
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "the program file name `{EXECUTABLE_NAME}` may only contain lower-case \
             letters, digits and `-`"
        );
    }

    #[test]
    fn project_file_extension_has_no_dot_and_is_lower_case() {
        assert!(
            !PROJECT_FILE_EXTENSION.is_empty(),
            "the project file extension is empty"
        );
        assert!(
            PROJECT_FILE_EXTENSION
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "the project file extension `{PROJECT_FILE_EXTENSION}` may only contain \
             lower-case letters and digits, without the leading dot"
        );
    }

    #[test]
    fn application_id_is_a_reverse_domain_name() {
        let parts: Vec<&str> = APPLICATION_ID.split('.').collect();
        assert!(
            parts.len() >= 3,
            "the application identifier `{APPLICATION_ID}` needs at least three \
             parts separated by dots, like `org.example.App`"
        );
        for part in parts {
            assert!(
                part.chars().next().is_some_and(|c| c.is_ascii_alphabetic()),
                "each part of `{APPLICATION_ID}` must start with a letter"
            );
            assert!(
                part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "each part of `{APPLICATION_ID}` may only contain letters, digits and `_`"
            );
        }
    }
}
