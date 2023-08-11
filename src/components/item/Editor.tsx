import AceEditor from 'react-ace'
import {useAPIStore} from "../../store/store";
import 'ace-builds/src-noconflict/mode-json'
import 'ace-builds/src-noconflict/mode-html'
import 'ace-builds/src-noconflict/mode-xml'
import 'ace-builds/src-noconflict/mode-plain_text'
import 'ace-builds/src-noconflict/theme-vibrant_ink'
import 'ace-builds/src-noconflict/theme-eclipse'
import {useMemo} from "react";

export const Editor = () => {
    const currentRequest = useAPIStore(state => state.currentRequest)
    const mode = useMemo(()=>{
        const candidate = currentRequest?.headers?.['content-type']?.split(';')[0].split('/')?.[1]
        if (!candidate || candidate === 'plain') {
            return 'plain_text'
        }
        return candidate
    },[currentRequest])

    return <AceEditor
        mode={mode}
        theme="monokai"
        className="aceEditor"
        fontSize={12.5}
        width="100%"
        showPrintMargin={true}
        showGutter={true}
        highlightActiveLine={false}
        value={currentRequest!.body}
        setOptions={{
            maxLines: 15,
            minLines: 10,
            wrap: true,
            tabSize: 2,
            showPrintMargin: false,
            useWorker: false,
        }}
    />
}
