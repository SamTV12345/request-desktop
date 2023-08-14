export interface ImplicitFlow {
    tokenName: string;
    callbackURL: string;
    authUrl: string;
    clientId: string;
    scope: string;
    state: string;
    clientAuthentication: "header"|"body"
}


export interface AuthorizationCodeFlow {
    tokenName: string;
    callbackURL: string;
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
    callbackURL: string;
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
