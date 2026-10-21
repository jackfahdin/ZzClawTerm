pub(in crate::features::pages::transfers) fn format_permissions_octal(mode: u32) -> String {
    format!("{:04o}", mode & 0o7777)
}
