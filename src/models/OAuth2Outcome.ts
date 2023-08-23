export interface OAuth2SucessOutcome {
    access_token: string,
    expires_in: number,
    scope: string,
    token_type: string,
    token_name:string,
    id?:string
}


export interface TokenLoadResult {
    access_token: string,
    token_name:string
}
export interface OAuth2FailureOutcome {
    message: string,
    description: string,
}


export interface Token extends OAuth2SucessOutcome {
    token_name: string,
    expires_at: number,
    scope: string,
    jti: string,
    iss: string,
    sub: string,
    typ: string,
    azp: string,
    acr: string,
    allowed_origins: string[],
    resource_access: {
        [key: string]: {
            roles: string[]
        }
    }
    clientHost: string,
    email_verified: boolean,
    preferred_username: string,
    clientAddress: string,
    client_id: string,
}

export interface TokenWithKey extends Token {
    key:string
}
