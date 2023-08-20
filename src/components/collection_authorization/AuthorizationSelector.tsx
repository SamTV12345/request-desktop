import * as Select from '@radix-ui/react-select';
import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {FC} from "react";
import {DisplayType, useAPIStore} from "../../store/store";
import {isItemsGroupDefinition} from "../../utils/utils";

type AuthorizationSelectorProps = {
    value: AuthorizationTypes,
    onChange: (value: AuthorizationTypes) => void
}


export const AuthorizationSelector:FC<AuthorizationSelectorProps> = ({value,onChange}) => {
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const currentItem = useAPIStore(state=>state.currentItem)

    const getAuthSelection = ()=>{
        if(currentCollection?.type === DisplayType.COLLECTION_TYPE){
            return currentCollection?.auth?.type
        }
        else if (isItemsGroupDefinition(currentItem)) {
            return currentItem?.auth?.type
        }
        else if(currentItem!==undefined){
            return currentItem?.request?.auth?.type
        }
    }

    return <div className="">
            <select value={getAuthSelection()} className="bg-basecol" onChange={v=>onChange(v.target.value as AuthorizationTypes)}>
                <option value={AuthorizationTypes.NOAUTH}>No Auth</option>
                <option value={undefined}>Inherit from parent</option>
                <option value={AuthorizationTypes.APIKEY}>API key</option>
                <option value={AuthorizationTypes.BEARER}>Bearer Token</option>
                <option value={AuthorizationTypes.AWSV4}>AWS Signature</option>
                <option value={AuthorizationTypes.Basic}>Basic Auth</option>
                <option value={AuthorizationTypes.DIGEST}>Digest Auth</option>
                <option value={AuthorizationTypes.HAWK}>Hawk Auth</option>
                <option value={AuthorizationTypes.NTLM}>NTLM Auth</option>
                <option value={AuthorizationTypes.OAUTH1}>OAuth 1.0</option>
                <option value={AuthorizationTypes.OAUTH2}>OAuth 2.0</option>
            </select>
    </div>
}
