import {ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {BodySelector, BodyType} from "./BodySelector";
import {RawBody} from "./bodyTypes/RawBody";
import {UrlEncodedBody} from "./bodyTypes/UrlEncodedBody";
import {FormDataBody} from "./bodyTypes/FormDataBody";
import {FileBody} from "./bodyTypes/FileBody";

export const BodyDecider = () => {
    const collection = useAPIStore(state => state.currentItem) as ItemDefinitionExtended

    const Body = ()=>{
         switch(collection?.request?.body?.mode){
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

    return <div>
        <BodySelector/>
        <Body/>
    </div>
}
