import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {useForm, useWatch} from "react-hook-form";
import {useMemo} from "react";
import {VariableDefinition} from "postman-collection";

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


    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        const filteredCollection =  currentCollection?.auth?.oauth2?.filter((v)=>v.key === key)
        if(filteredCollection === undefined|| !filteredCollection[0]){
            return defaultValue
        }
        return filteredCollection[0].value
    }

    const { register, handleSubmit, watch,
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

    const watchShowAge = watch(['grant_type']); // you can supply default value as second argument

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

    return <div>
            <div>
                <div>Current token:</div>
                This token is only available to you. Sync the token to let collaborators on this request use it.
                <div className="grid grid-cols-2 gap-5 pt-2">
                    <div>Token</div>
                    <select className="bg-basecol p-1">
                        <option>Token Name</option>
                    </select>
                    <div>Use token type</div>
                    <select className="bg-basecol p-1">
                        <option>Access Token</option>
                    </select>
                    <div>Header Prefix</div>
                    <input className="bg-basecol p-1" defaultValue="Bearer"/>
                    <div>Auto refresh token</div>
                    <input type="checkbox" className="bg-basecol p-1"/>
                    <div>Share token</div>
                    <input type="checkbox" className="bg-basecol p-1"/>
                </div>
            </div>
        <h2 className="font-bold mt-5">Configure New Token</h2>
        <form onSubmit={handleSubmit(populateOAuth2Auth)} className="grid grid-cols-2 gap-5 text-white">
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
            { (watchShowAge[0] !== OAuth2Flow.PASSWORD_CREDENTIALS) && (watchShowAge[0] !== OAuth2Flow.CLIENT_CREDENTIALS) &&
                <>
                    <label>Callback URL:</label>
                    <input {...register('redirect_uri')} className="bg-basecol p-1"/>
                </>
            }
            { (watchShowAge[0] !== OAuth2Flow.PASSWORD_CREDENTIALS) && (watchShowAge[0] !== OAuth2Flow.CLIENT_CREDENTIALS) &&
                <>
                    <label>Auth URL:</label>
                    <input {...register('authUrl')} className="bg-basecol p-1"/>
                </>
            }
            { (watchShowAge[0] !== OAuth2Flow.IMPLICIT)&&
                <>
                <label>Access Token URL:</label>
                <input {...register('accessTokenUrl')} className="bg-basecol p-1"/>
                </>
        }
                <label>Client ID:</label>
                <input {...register('clientId')} className="bg-basecol p-1"/>
            {(watchShowAge[0] !== OAuth2Flow.IMPLICIT) && <>
                <label>Client Secret:</label>
                <input {...register('clientSecret')} className="bg-basecol p-1"/>
                </>
            }

            {
                (watchShowAge[0] === OAuth2Flow.AUTHORIZATION_CODE_PKCE) && <>
                    <label>Code Challenge Method</label>
                    <select className="bg-basecol p-1" {...register('challengeAlgorithm')}>
                        <option value="S256">SHA-256</option>
                        <option value="plain">Plain</option>
                    </select>
                </>
            }
            {(watchShowAge[0] === OAuth2Flow.AUTHORIZATION_CODE_PKCE) && <>
                <label>Code Verifier:</label>
                <input {...register('code_verifier')} className="bg-basecol p-1"/>
            </>
            }





            {
                (watchShowAge[0] === OAuth2Flow.PASSWORD_CREDENTIALS) &&
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
            <button type="submit">Speichern</button>
    </form></div>


}