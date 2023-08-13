import {useAPIStore} from "../../store/store";
import {useForm} from "react-hook-form";

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
    addTokenTo: string
}

export const OAuth2Authentication = () => {
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)

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
            grant_type: getKey('grant_type', ""),
            authUrl: getKey('authUrl', ""),
            redirect_uri: getKey('redirect_uri', ""),
            client_authentication: getKey('client_authentication', ""),
            password: getKey('password', ""),
            username: getKey('username', ""),
            useBrowser: getKey('useBrowser', "false") === "true",
            code_verifier: getKey('code_verifier', ""),
            addTokenTo: getKey('addTokenTo', "")
        }
    });

    const populateOAuth2Auth = (data: OAuth2Data)=>{

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


    </form></div>


}
