import {FC} from "react";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {useForm} from "react-hook-form";
import {VariableDefinition} from "postman-collection";
import {APiKeyAuthentication} from "./APiKeyAuthentication";
import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {BearerAuthentication} from "./BearerAuthentication";
import {BasicAuthentication} from "./BasicAuthentication";
import {OAuth2Authentication} from "./OAuth2Authentication";


type AuthorizationContentProps = {

}


export const AuthorizationContent:FC<AuthorizationContentProps> = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)

    console.log(currentCollection?.auth?.type)
    const AuthContentSwitcher = ()=>{
        return <div>
            {
                currentCollection?.auth?.type === AuthorizationTypes.APIKEY &&<APiKeyAuthentication/>||
                currentCollection?.auth?.type === AuthorizationTypes.BEARER &&<BearerAuthentication/>||
                currentCollection?.auth?.type === AuthorizationTypes.Basic && <BasicAuthentication/>||
                currentCollection?.auth?.type === AuthorizationTypes.NOAUTH && <div>No Auth</div>||
                currentCollection?.auth?.type === AuthorizationTypes.OAUTH2 && <OAuth2Authentication/>
            }
        </div>
    }



    return <div>
        <AuthContentSwitcher/>
    </div>

}
