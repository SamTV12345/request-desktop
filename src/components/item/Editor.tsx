import {useAPIStore} from "../../store/store";
import {useMemo} from "react";
import MonacoEditor, { DiffEditor, useMonaco, loader } from '@monaco-editor/react';
export const Editor = () => {
    const currentRequest = useAPIStore(state => state.currentRequest)
    const mode = useMemo(()=>{
        const candidate = currentRequest?.headers?.['content-type']?.split(';')[0].split('/')?.[1]
        if (!candidate || candidate === 'plain') {
            return 'plain_text'
        }
        return candidate
    },[currentRequest])

    return <MonacoEditor
        defaultLanguage={mode} value={currentRequest?.body?.toString() || ""} theme="vs-dark" options={{
            minimap :{
                enabled: false
            },
        codeLens: false,
        readOnly: true,
        quickSuggestions: false,
        wordWrap: "on",
        wrappingStrategy: "advanced",
    }}
    />
}
