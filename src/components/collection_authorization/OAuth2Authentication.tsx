import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {useForm, useWatch} from "react-hook-form";
import {useMemo} from "react";
import {VariableDefinition} from "postman-collection";
import { emit, listen } from '@tauri-apps/api/event'
import {invoke} from '@tauri-apps/api/tauri'
import {
    AuthorizationCodeFlow,
    AuthorizationCodeFlowPKCE,
    ClientCredentialsFlow,
    ImplicitFlow, PasswordCredentialsFlow
} from "../../models/OAuth2Models";
import {OAuth2Loader} from "./OAuth2Loader";
import {OAuth2SucessOutcome} from "../../models/OAuth2Outcome";
import {TokenManager} from "./TokenManager";
import {TokenSelector} from "./TokenSelector";
import {copyToClipboard} from "../../utils/utils";

type OAuth2Data = {
    tokenName: string,
    accessTokenUrl: string,
    scope: string,
    clientSecret: string,
    clientId: string,
    grant_type: string,
    authUrl: string,
    redirect_uri: string,
    client_authentication: string,
    password: string,
    username: string,
    useBrowser: boolean,
    code_verifier: string,
    addTokenTo: string,
    challengeAlgorithm: string
}


export enum OAuth2Flow {
    PASSWORD_CREDENTIALS="password_credentials",
    CLIENT_CREDENTIALS="client_credentials",
    IMPLICIT="implicit",
    AUTHORIZATION_CODE="authorization_code",
    AUTHORIZATION_CODE_PKCE="authorization_code_with_pkce"
}


export const OAuth2Authentication = () => {
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const setOpenSuccessOAuth2 = useAPIStore(state=>state.setOAuth2Screen)
    const setPayload = useAPIStore(state=>state.setOAuth2Outcome)
    const selectedToken = useAPIStore(state=>state.selectedToken)

    listen('oauth2-callback', (event)=>{
        console.log(event)
    })
        .then(c=>console.log(c))
        .catch(c=>console.log(c))

    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        const filteredCollection =  currentCollection?.auth?.oauth2?.filter((v)=>v.key === key)
        if(filteredCollection === undefined|| !filteredCollection[0]){
            return defaultValue
        }
        return filteredCollection[0].value
    }

    const { register, getValues, handleSubmit, watch,
        formState: { errors } } = useForm<OAuth2Data>({
        defaultValues: {
            tokenName: getKey('tokenName', ""),
            accessTokenUrl: getKey('accessTokenUrl', ""),
            scope: getKey('scope', ""),
            clientSecret: getKey('clientSecret', ""),
            clientId: getKey('clientId', ""),
            grant_type: getKey('grant_type', OAuth2Flow.CLIENT_CREDENTIALS),
            authUrl: getKey('authUrl', ""),
            redirect_uri: getKey('redirect_uri', ""),
            client_authentication: getKey('client_authentication', "header"),
            challengeAlgorithm: getKey('challengeAlgorithm', "S256"),
            password: getKey('password', ""),
            username: getKey('username', ""),
            useBrowser: getKey('useBrowser', "false") === "true",
            code_verifier: getKey('code_verifier', ""),
            addTokenTo: getKey('addTokenTo', "header")
        }
    })

    const grant_type_selection = watch(['grant_type']); // you can supply default value as second argument

    const doRequest: ()=>Promise<OAuth2SucessOutcome|undefined> = async ()=>{
        let val = getValues()
        console.log(val)
            switch (grant_type_selection[0]){
            case OAuth2Flow.CLIENT_CREDENTIALS:
                let config1: ClientCredentialsFlow = {
                    state: "",
                    accessTokenUrl: val.accessTokenUrl,
                    clientId: val.clientId,
                    scope: val.scope,
                    clientAuthentication: val.client_authentication as "header" | "body",
                    clientSecret: val.clientSecret,
                    tokenName: val.tokenName
                }

                return invoke<OAuth2SucessOutcome>('get_oauth2_token', {
                    config: config1
                })
            case OAuth2Flow.AUTHORIZATION_CODE_PKCE:
                console.log("Authorization Code")
                let config2: AuthorizationCodeFlowPKCE = {
                    authUrl: val.authUrl,
                    clientAuthentication: val.client_authentication as "header" | "body",
                    accessTokenUrl: val.accessTokenUrl,
                    clientId: val.clientId,
                    clientSecret: val.clientSecret,
                    codeVerifier: val.code_verifier,
                    callbackUrl: val.redirect_uri,
                    scope: val.scope,
                    codeChallengeMethod: val.challengeAlgorithm as "S256" | "plain",
                    state: "",
                    codeChallenge: val.challengeAlgorithm,
                    tokenName: val.tokenName
                }
                return invoke<OAuth2SucessOutcome>('get_oauth2_token', {
                    config: config2
                })
            case OAuth2Flow.IMPLICIT:
                let config3: ImplicitFlow = {
                    state: "",
                    clientAuthentication: val.client_authentication as "header" | "body",
                    authUrl: val.authUrl,
                    clientId: val.clientId,
                    scope: val.scope,
                    callbackUrl: val.redirect_uri,
                    tokenName: val.tokenName
                }
                return invoke<OAuth2SucessOutcome>('get_oauth2_token', {
                    config: config3
                })
            case OAuth2Flow.AUTHORIZATION_CODE:
                let config4: AuthorizationCodeFlow = {
                    state: "",
                    clientAuthentication: val.client_authentication as "header" | "body",
                    authUrl: val.authUrl,
                    clientId: val.clientId,
                    clientSecret: val.clientSecret,
                    scope: val.scope,
                    callbackUrl: val.redirect_uri,
                    accessTokenUrl: val.accessTokenUrl,
                    tokenName: val.tokenName
                }
                console.log("Authorization Code", config4)
                return invoke<OAuth2SucessOutcome>('get_oauth2_token', {
                    config: config4
                })
            case OAuth2Flow.PASSWORD_CREDENTIALS:
                console.log("password credentials")
                let config5: PasswordCredentialsFlow = {
                    state: "",
                    clientAuthentication: val.client_authentication as "header" | "body",
                    clientId: val.clientId,
                    clientSecret: val.clientSecret,
                    scope: val.scope,
                    accessTokenUrl: val.accessTokenUrl,
                    tokenName: val.tokenName,
                    password: val.password,
                    username: val.username
                }
                console.log(config5)
                return invoke<OAuth2SucessOutcome>('get_oauth2_token', {
                    config: config5
                })
            default: new Promise<OAuth2SucessOutcome>((resolve, reject)=>{
                reject("Unknown grant type")
            })
        }
    }

    const populateOAuth2Auth = (data: OAuth2Data)=>{
        const newAuth = [{
            key: "tokenName",
            type: "string",
            value: data.tokenName

        },
            {
                key: "accessTokenUrl",
                type: "string",
                value: data.accessTokenUrl
            },
            {
                key: "scope",
                type: "string",
                value: data.scope
            },
            {
                key: "clientSecret",
                type: "string",
                value: data.clientSecret
            },
            {
                key: "clientId",
                type: "string",
                value: data.clientId
            },
            {
                key: "grant_type",
                type: "string",
                value: data.grant_type
            },
            {
                key: "authUrl",
                type: "string",
                value: data.authUrl
            },
            {
                key: "redirect_uri",
                type: "string",
                value: data.redirect_uri
            },
            {
                key: "client_authentication",
                type: "string",
                value: data.client_authentication
            },
            {
                key: "challengeAlgorithm",
                type: "string",
                value: data.challengeAlgorithm
            },
            {
                key: "password",
                type: "string",
                value: data.password
            },
            {
                key: "username",
                type: "string",
                value: data.username
            },
            {
                key: "useBrowser",
                type: "string",
                value: data.useBrowser.toString()
            },
            {
                key: "code_verifier",
                type: "string",
                value: data.code_verifier
            },
            {
                key: "addTokenTo",
                type: "string",
                value: data.addTokenTo
            }
        ] as VariableDefinition[]

        const clonedCollection:CollectionDefinitionExtended = {
            ...currentCollection!,
            auth: {
                ...currentCollection?.auth,
                type: "oauth2",
                oauth2: newAuth
            }
        }
        updateCollection(clonedCollection)
        saveCollection()
    }

    return <div className="">
        <OAuth2Loader/>
        <TokenManager/>
            <div>
                <div>Current token:</div>
                This token is only available to you. Sync the token to let collaborators on this request use it.
                <div className="grid grid-cols-2 gap-5 pt-2">
                    <div>Token</div>
                    <div className="block">
                        <TokenSelector/>
                    </div>
                    {selectedToken&& <>
                        <div>Use token type</div>
                        <div className="grid grid-cols-[1fr_auto] gap-3">
                            <input readOnly className="bg-basecol p-1  pr-5 text-ellipsis"
                                   value={selectedToken?.access_token}/>
                            <button className="material-symbols-outlined self-center active:scale-95 "
                                    onClick={() => copyToClipboard(selectedToken.access_token)}>content_copy
                            </button>
                        </div>
                    </>
                    }
                    <div>Header Prefix</div>
                    <input className="bg-basecol p-1" defaultValue="Bearer"/>
                    <div>Auto refresh token</div>
                    <input type="checkbox" className="bg-basecol p-1"/>
                    <div>Share token</div>
                    <input type="checkbox" className="bg-basecol p-1"/>
                </div>
            </div>
        <h2 className="font-bold mt-5">Configure New Token</h2>
        <form onSubmit={handleSubmit(populateOAuth2Auth)} className="grid grid-cols-2 gap-5 text-white mb-2">
                <label>Token Name:</label>
                <input {...register('tokenName')} className="bg-basecol p-1"/>
                <label>Grant Type:</label>
            <select {...register('grant_type')} className="bg-basecol p-1">
                <option value={OAuth2Flow.AUTHORIZATION_CODE}>Authorization Code</option>
                <option value={OAuth2Flow.AUTHORIZATION_CODE_PKCE}>Authorization Code with PKCE</option>
                <option value={OAuth2Flow.IMPLICIT}>Implicit</option>
                <option value={OAuth2Flow.PASSWORD_CREDENTIALS}>Password Credentials</option>
                <option value={OAuth2Flow.CLIENT_CREDENTIALS}>Client Credentials</option>
            </select>
            { (grant_type_selection[0] !== OAuth2Flow.PASSWORD_CREDENTIALS) && (grant_type_selection[0] !== OAuth2Flow.CLIENT_CREDENTIALS) &&
                <>
                    <label>Callback URL:</label>
                    <input {...register('redirect_uri')} className="bg-basecol p-1"/>
                </>
            }
            { (grant_type_selection[0] !== OAuth2Flow.PASSWORD_CREDENTIALS) && (grant_type_selection[0] !== OAuth2Flow.CLIENT_CREDENTIALS) &&
                <>
                    <label>Auth URL:</label>
                    <input {...register('authUrl')} className="bg-basecol p-1"/>
                </>
            }
            { (grant_type_selection[0] !== OAuth2Flow.IMPLICIT)&&
                <>
                <label>Access Token URL:</label>
                <input {...register('accessTokenUrl')} className="bg-basecol p-1"/>
                </>
        }
                <label>Client ID:</label>
                <input {...register('clientId')} className="bg-basecol p-1"/>
            {(grant_type_selection[0] !== OAuth2Flow.IMPLICIT) && <>
                <label>Client Secret:</label>
                <input {...register('clientSecret')} className="bg-basecol p-1"/>
                </>
            }

            {
                (grant_type_selection[0] === OAuth2Flow.AUTHORIZATION_CODE_PKCE) && <>
                    <label>Code Challenge Method</label>
                    <select className="bg-basecol p-1" {...register('challengeAlgorithm')}>
                        <option value="S256">SHA-256</option>
                        <option value="plain">Plain</option>
                    </select>
                </>
            }
            {(grant_type_selection[0] === OAuth2Flow.AUTHORIZATION_CODE_PKCE) && <>
                <label>Code Verifier:</label>
                <input {...register('code_verifier')} className="bg-basecol p-1"/>
            </>
            }

            {
                (grant_type_selection[0] === OAuth2Flow.PASSWORD_CREDENTIALS) &&
                <>
                    <label>Username:</label>
                    <input {...register('username')} className="bg-basecol p-1"/>
                    <label>Password:</label>
                    <input {...register('password')} className="bg-basecol p-1"/>
                </>
            }
                <>
                    <label>Scope:</label>
                    <input {...register('scope')} className="bg-basecol p-1"/>
                </>
            <label>Client Authentication:</label>
            <select {...register('client_authentication')} className="bg-basecol p-1">
                <option value="header">Send as Basic Auth header</option>
                <option value="body">Send as POST body parameter</option>
            </select>
            <button type="submit" className="bg-mustard-600">Save</button>
    </form>
        <button className="bg-mustard-600 pl-2 pr-2 pt-1 pb-1 rounded" onClick={async () => {
            await doRequest()
                .then((c)=>{
                    setPayload({...c, token_name: getValues().tokenName!} as OAuth2SucessOutcome)
                    setOpenSuccessOAuth2(true)
                })
                .catch(c=>{
                    setPayload(c)
                    setOpenSuccessOAuth2(true)})
        }}>Get token</button>
    </div>


}
