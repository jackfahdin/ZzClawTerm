mod aliyun;
mod github_gist_auth;
mod google_drive;
mod helpers;
mod onedrive;
mod s3;
mod snippet;
mod webdav;

pub use aliyun::NativeAliyunDriveRemote;
pub(crate) use github_gist_auth::run_github_gist_device_flow;
pub use google_drive::NativeGoogleDriveRemote;
pub use onedrive::NativeOneDriveRemote;
pub use s3::NativeS3Remote;
pub use snippet::NativeSnippetHttpClient;
pub use webdav::NativeWebdavRemote;

#[cfg(test)]
mod tests;
