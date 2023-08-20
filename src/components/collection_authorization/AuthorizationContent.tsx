import {FC} from "react";
import {DisplayType, useAPIStore} from "../../store/store";
import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {isItemsGroupDefinition} from "../../utils/utils";
import {APiKeyAuthentication} from "./APiKeyAuthentication";
import {BearerAuthentication} from "./BearerAuthentication";
import {BasicAuthentication} from "./BasicAuthentication";
import {OAuth2Authentication} from "./OAuth2Authentication";


type AuthorizationContentProps = {

}


export const AuthorizationContent:FC<AuthorizationContentProps> = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const currentItem = useAPIStore(state=>state.currentItem)


    const isSingleTypeAndNoAuth = ()=>{
        if(isItemsGroupDefinition(currentItem)){
            return currentCollection?.type === DisplayType.SINGLE_TYPE && currentItem?.auth?.type == undefined
        }
        else if (currentItem!==undefined && currentCollection?.type === DisplayType.SINGLE_TYPE){
            return currentItem!.request?.auth?.type == undefined
        }
    }

    const AuthContentSwitcher = ()=>{

        const isVisible = (authType:AuthorizationTypes|undefined)=>{
            if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
                return currentCollection?.auth?.type === authType
            }
            else if (isItemsGroupDefinition(currentItem)) {
                return currentItem?.auth?.type === authType
            }
            else if (currentItem!==undefined) {
                return currentItem?.request?.auth?.type === authType
            }
            else if (authType === undefined)
                return false
        }
        return <div>
            {
                isSingleTypeAndNoAuth() && <div>Inherit from Parent</div>||
                isVisible(AuthorizationTypes.APIKEY) &&<APiKeyAuthentication/>||
                isVisible(AuthorizationTypes.BEARER) &&<BearerAuthentication/>||
                isVisible(AuthorizationTypes.Basic) && <BasicAuthentication/>||
                isVisible(AuthorizationTypes.NOAUTH) && <div>No Auth</div>||
                isVisible(AuthorizationTypes.OAUTH2) && <OAuth2Authentication/>
            }
        </div>
    }



    return <div className="w-full">
        <AuthContentSwitcher/>
    </div>

}
