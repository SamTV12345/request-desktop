use oauth2::basic::{BasicClient, BasicErrorResponseType, BasicTokenResponse, BasicTokenType};
use oauth2::{AccessToken, AuthorizationCode, AuthUrl, Client, ClientId, ClientSecret, CsrfToken, DeviceAuthorizationUrl, EmptyExtraTokenFields, ErrorResponseType, PkceCodeChallenge, RedirectUrl, RequestTokenError, ResourceOwnerPassword, ResourceOwnerUsername, Scope, StandardDeviceAuthorizationResponse, StandardErrorResponse, TokenResponse, TokenUrl};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Window};

use oauth2::reqwest::async_http_client;
use crate::oauth2_error::OAuth2Error;
use crate::oauth::OAuth2Type::RefreshToken;
use oauth2::reqwest::http_client;


pub async fn handle_oauth(window: &Window, config: OAuth2Type, app_state: AppHandle) ->Result<BasicTokenResponse, OAuth2Error>{
    println!("Config: {:?}", config);
    return match config {
        OAuth2Type::Implicit(s) => {
            let client =
                BasicClient::new(
                    ClientId::new("client_id".to_string()),
                    Some(ClientSecret::new("client_secret".to_string())),
                    AuthUrl::new("http://authorize".to_string()).unwrap(),
                    None
                );

            let (auth_url, csrf_token) = client
                .authorize_url(CsrfToken::new_random)
                .use_implicit_flow()
                .url();

            return Ok(BasicTokenResponse::new(AccessToken::new("token".to_string()),
                                              BasicTokenType::Mac,
            EmptyExtraTokenFields{

            }))
        }
        OAuth2Type::AuthorizationCode(a) => {



           Err(OAuth2Error::new("Not implemented".to_string(), Option::from("test".to_string())))
        }
        OAuth2Type::ClientCredentials(a) => {
            let client =
                BasicClient::new(
                    ClientId::new(a.client_id.clone()),
                    Some(ClientSecret::new(a.client_secret.clone())),
                    AuthUrl::new(a.access_token_url.clone()).unwrap(),
                    Some(TokenUrl::new(a.access_token_url.clone()).unwrap())
                );

            client.exchange_client_credentials()
                .request_async(async_http_client)
                .await
                .map_err(|e| {
                        OAuth2Error::from_basic_error(e)
                })
        }
        OAuth2Type::Password(a) => {
            let client =
                BasicClient::new(
                    ClientId::new(a.client_id.clone()),
                    Some(ClientSecret::new(a.client_secret.clone())),
                    AuthUrl::new(a.access_token_url.clone()).unwrap(),
                    Some(TokenUrl::new(a.access_token_url.clone()).unwrap())
                );

            client.exchange_password(&ResourceOwnerUsername::new(a.username),
                                                       &ResourceOwnerPassword::new(a.password))
                .request_async(async_http_client).await
                .map_err(|e| {
                    OAuth2Error::from_basic_error(e)
                })


        }
        OAuth2Type::DeviceCode => {
            let device_auth_url = DeviceAuthorizationUrl::new("http://deviceauth".to_string()).unwrap();
            let client =
                BasicClient::new(
                    ClientId::new("client_id".to_string()),
                    Some(ClientSecret::new("client_secret".to_string())),
                    AuthUrl::new("http://authorize".to_string()).unwrap(),
                    Some(TokenUrl::new("http://token".to_string()).unwrap()),
                )
                    .set_device_authorization_url(device_auth_url);

            let details: StandardDeviceAuthorizationResponse = client
                .exchange_device_code()
                .map_err(|e| {
                    OAuth2Error::from_configuration_error(e)
                })?
                .add_scope(Scope::new("read".to_string()))
                .request(http_client)
                .map_err(|e| {
                    OAuth2Error::from_basic_error(e)
                })?;



            client
                    .exchange_device_access_token(&details)
                    .request(http_client, std::thread::sleep, None)
                .map_err(|e| {
                    OAuth2Error::from_device_error(e)
                })

        }
        RefreshToken(a) => {
            use oauth2::RefreshToken;
            let client =
                BasicClient::new(
                    ClientId::new("client_id".to_string()),
                    Some(ClientSecret::new("client_secret".to_string())),
                    AuthUrl::new("http://authorize".to_string()).unwrap(),
                    Some(TokenUrl::new("http://token".to_string()).unwrap()),
                );
            client.exchange_refresh_token(&RefreshToken::new(a.token_name))
                .request_async(async_http_client)
                .await
                .map_err(|e| {
                    OAuth2Error::from_basic_error(e)
                })

        }
        OAuth2Type::AuthorizationCodeWithPKCE(a) => {
            let client =
                BasicClient::new(
                    ClientId::new("client_id".to_string()),
                    Some(ClientSecret::new("client_secret".to_string())),
                    AuthUrl::new("http://authorize".to_string()).unwrap(),
                    Some(TokenUrl::new("http://token".to_string()).unwrap())
                )
                    // Set the URL the user will be redirected to after the authorization process.
                    .set_redirect_uri(RedirectUrl::new("http://redirect".to_string()).unwrap());
            // Generate a PKCE challenge.
            let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

            let (auth_url, csrf_token) = client
                .authorize_url(CsrfToken::new_random)
                // Set the desired scopes.
                .add_scope(Scope::new("read".to_string()))
                .add_scope(Scope::new("write".to_string()))
                // Set the PKCE code challenge.
                .set_pkce_challenge(pkce_challenge)
                .url();

             Ok(client
                .exchange_code(AuthorizationCode::new("some authorization code".to_string()))
                // Set the PKCE code verifier.
                .set_pkce_verifier(pkce_verifier)
                .request_async(async_http_client)
                    .await
                    .map_err(|e| {
                        OAuth2Error::from_basic_error(e)
                    })?)

        }
    }
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum OAuth2Type {
    Implicit(ImplicitFlow),
    AuthorizationCode(AuthorizationCodeFlow),
    AuthorizationCodeWithPKCE(AuthorizationCodeFlowWithPKCE),
    Password(PasswordFlow),
    DeviceCode,
    ClientCredentials(ClientCredentialsFlow),
    RefreshToken(RefreshTokenFlow)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImplicitFlow {
    token_name: String,
    callback_url:String,
    auth_url: String,
    client_id:String,
    scope: String,
    state: String,
    client_authentication: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeFlow {
    token_name: String,
    callback_url:String,
    auth_url: String,
    access_token_url: String,
    client_id:String,
    scope: String,
    state: String,
    client_authentication: String
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeFlowWithPKCE {
    token_name: String,
    callback_url:String,
    auth_url: String,
    access_token_url: String,
    client_id:String,
    client_secret: String,
    scope: String,
    state: String,
    client_authentication: String,
    code_challenge_method: String,
    code_challenge: String
}

#[derive(Serialize, Deserialize,Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClientCredentialsFlow {
    token_name: String,
    access_token_url: String,
    client_id:String,
    client_secret: String,
    scope: String,
    state: String,
    client_authentication: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PasswordFlow {
    token_name: String,
    access_token_url: String,
    client_id:String,
    client_secret: String,
    username: String,
    password: String,
    scope: String,
    client_authentication: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenFlow {
    token_name: String
}
