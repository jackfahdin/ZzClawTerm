//! Runtime SSH credential selection and deferred persistence.
mod runtime;
mod state;
mod view;

pub(in crate::features) use state::{
    CredentialResponseSender, RuntimeAuthenticationContext, RuntimeAuthenticationForm,
};
