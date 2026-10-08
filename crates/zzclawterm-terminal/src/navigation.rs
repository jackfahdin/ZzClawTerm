//! Unmodified cursor keys shared by physical keyboard and shell editing adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationKey {
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
}

pub fn navigation_key_bytes(key: NavigationKey, application_cursor: bool) -> &'static [u8] {
    match (application_cursor, key) {
        (false, NavigationKey::Up) => b"\x1b[A",
        (false, NavigationKey::Down) => b"\x1b[B",
        (false, NavigationKey::Right) => b"\x1b[C",
        (false, NavigationKey::Left) => b"\x1b[D",
        (false, NavigationKey::Home) => b"\x1b[H",
        (false, NavigationKey::End) => b"\x1b[F",
        (true, NavigationKey::Up) => b"\x1bOA",
        (true, NavigationKey::Down) => b"\x1bOB",
        (true, NavigationKey::Right) => b"\x1bOC",
        (true, NavigationKey::Left) => b"\x1bOD",
        (true, NavigationKey::Home) => b"\x1bOH",
        (true, NavigationKey::End) => b"\x1bOF",
    }
}

#[cfg(test)]
mod tests {
    use super::{NavigationKey, navigation_key_bytes};

    #[test]
    fn cursor_modes_encode_csi_and_ss3_for_every_navigation_key() {
        for (key, final_byte) in [
            (NavigationKey::Up, b'A'),
            (NavigationKey::Down, b'B'),
            (NavigationKey::Right, b'C'),
            (NavigationKey::Left, b'D'),
            (NavigationKey::Home, b'H'),
            (NavigationKey::End, b'F'),
        ] {
            assert_eq!(navigation_key_bytes(key, false), &[0x1b, b'[', final_byte]);
            assert_eq!(navigation_key_bytes(key, true), &[0x1b, b'O', final_byte]);
        }
    }
}
