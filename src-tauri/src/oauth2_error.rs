use std::error::Error as StdErr;
use oauth2::basic::BasicErrorResponse;
use oauth2::{ConfigurationError, DeviceCodeErrorResponse, ErrorResponse, RequestTokenError};
use oauth2::reqwest::Error;
use reqwest::Error as ReqwestError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OAuth2Error{
    message: String,
    description: Option<String>
}

impl OAuth2Error{
    pub fn new(message: String, description: Option<String>) -> OAuth2Error{
        OAuth2Error{
            message,
            description
        }
    }

    pub fn from_basic_error(error: RequestTokenError<Error<ReqwestError>, BasicErrorResponse>) -> OAuth2Error{
        match error {
            RequestTokenError::ServerResponse(e) => {
                let error = e.error();
                println!("Error: {:?}", e);
                OAuth2Error::new(error.to_string(), e.error_description().map(|s| s.to_string()))
            }
            RequestTokenError::Request(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), e.source().map(|s| s.to_string()))
            }
            RequestTokenError::Parse(e, ..) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), e.source().map(|s| s.to_string()))
            }
            RequestTokenError::Other(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), Option::from(e))
            }
        }
    }

    pub fn from_device_error(error: RequestTokenError<Error<ReqwestError>, DeviceCodeErrorResponse>) -> OAuth2Error{
        match error {
            RequestTokenError::ServerResponse(e) => {
                let error = e.error();
                println!("Error: {:?}", e);
                OAuth2Error::new(error.to_string(), e.error_description().map(|s| s.to_string()))
            }
            RequestTokenError::Request(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), e.source().map(|s| s.to_string()))
            }
            RequestTokenError::Parse(e, ..) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), e.source().map(|s| s.to_string()))
            }
            RequestTokenError::Other(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), Option::from(e))
            }
        }
    }

    pub fn from_configuration_error(error: ConfigurationError) -> OAuth2Error{
        match error {
            ConfigurationError::MissingUrl(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), error.source().map(|s| s.to_string()))
            }
            ConfigurationError::InsecureUrl(e) => {
                println!("Error: {:?}", e.to_string());
                OAuth2Error::new(e.to_string(), Option::from(e.to_string()))
            }

            _ => {
                OAuth2Error::new(error.to_string(), Option::from(error).map(|s| s.to_string()))
            }
        }
    }
}
