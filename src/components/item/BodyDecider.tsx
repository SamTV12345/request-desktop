import {ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {BodySelector, BodyType} from "./BodySelector";
import {RawBody} from "./bodyTypes/RawBody";
import {UrlEncodedBody} from "./bodyTypes/UrlEncodedBody";
import {FormDataBody} from "./bodyTypes/FormDataBody";
import {FileBody} from "./bodyTypes/FileBody";
import {useMemo} from "react";

export const BodyDecider = () => {
    const collection = useAPIStore(state => state.currentItem) as ItemDefinitionExtended
    const mode = useMemo(()=>{
        return collection?.request?.body?.mode
    },[collection?.request?.body?.mode])
    const Body = ()=>{
         switch(mode){
            case BodyType.raw:
                return <RawBody/>
            case BodyType.urlencoded:
                return <UrlEncodedBody/>
            case BodyType.formdata:
                return <FormDataBody/>
            case BodyType.file:
                return <FileBody/>
            default:
                return <div></div>
        }
    }

    return <div className="h-52">
        <BodySelector/>
        <Body/>
    </div>
}
