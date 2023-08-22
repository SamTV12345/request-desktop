import {Editor} from "./Editor";
import {useMemo} from "react";
import {useAPIStore} from "../../store/store";

export const BodyResponseEditor = ()=>{
    const currentRequest = useAPIStore(state => state.currentRequest)
    const mode = useMemo(()=>{
        const candidate = currentRequest?.headers?.['content-type']?.split(';')[0].split('/')?.[1]
        if (!candidate || candidate === 'plain') {
            return 'plain_text'
        }
        return candidate
    },[currentRequest])

    const prettyValue = useMemo(()=>{
        if(!currentRequest?.body){
            return ""
        }
        try{
            return JSON.stringify(JSON.parse(currentRequest?.body?.toString()!), null, 2)
        }
        catch(e){
            return currentRequest?.body?.toString()
        }
    },[])


    return <Editor readonly={true} value={prettyValue} onChange={()=>{}} mode={mode}/>
}
