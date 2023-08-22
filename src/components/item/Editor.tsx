import {useAPIStore} from "../../store/store";
import {FC, useMemo, useState} from "react";
import MonacoEditor, { DiffEditor, useMonaco, loader } from '@monaco-editor/react';
import {useDebounce} from "../../hooks/useDebounce";

type EditorProps = {
    readonly: boolean
    value: string
    onChange: (value: string|undefined)=>void
    mode: string
}

export const Editor:FC<EditorProps> = ({readonly,mode,value,onChange}) => {

    return <MonacoEditor
        onChange={onChange}
        defaultLanguage={mode} value={value || ""} theme="vs-dark" options={{
            minimap :{
                enabled: false
            },
        codeLens: false,
        readOnly: readonly,
        quickSuggestions: false,
        wordWrap: "on",
        wrappingStrategy: "advanced",
    }}
    />
}
