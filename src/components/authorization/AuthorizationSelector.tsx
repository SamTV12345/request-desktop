import * as Select from '@radix-ui/react-select';
import {AuthorizationTypes} from "../../models/AuthorizationTypes";
import {FC} from "react";

type AuthorizationSelectorProps = {
    value: AuthorizationTypes,
    onChange: (value: AuthorizationTypes) => void
}


export const AuthorizationSelector:FC<AuthorizationSelectorProps> = ({value,onChange}) => {

    return <div className="">
            <select value={value} className="bg-basecol" onChange={v=>onChange(v.target.value as AuthorizationTypes)}>
                <option value={AuthorizationTypes.NOAUTH}>No Auth</option>
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
