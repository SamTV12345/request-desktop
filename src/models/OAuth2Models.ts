export interface ImplicitFlow {
    tokenName: string;
    callbackUrl: string;
    authUrl: string;
    clientId: string;
    scope: string;
    state: string;
    clientAuthentication: "header"|"body"
}


export interface AuthorizationCodeFlow {
    tokenName: string;
    callbackUrl: string;
    authUrl: string;
    accessTokenUrl: string;
    clientId: string;
    clientSecret: string;
    scope: string;
    state: string;
    clientAuthentication: "header"|"body"
}

export interface AuthorizationCodeFlowPKCE {
    tokenName: string;
    callbackUrl: string;
    authUrl: string;
    accessTokenUrl: string;
    clientId: string;
    clientSecret: string;
    scope: string;
    state: string;
    clientAuthentication: "header"|"body"
    codeChallengeMethod: "S256"|"plain"
    codeVerifier: string;
    codeChallenge: string;
}

export interface ClientCredentialsFlow {
    tokenName: string;
    accessTokenUrl: string;
    clientId: string;
    clientSecret: string;
    scope: string;
    clientAuthentication: "header"|"body"
    state: string;
}

export interface PasswordCredentialsFlow {
    tokenName: string;
    accessTokenUrl: string;
    clientId: string;
    clientSecret: string;
    scope: string;
    clientAuthentication: "header"|"body"
    state: string;
    username: string;
    password: string;
}
