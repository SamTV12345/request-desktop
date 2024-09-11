import {useAPIStore} from "../../store/store";
import {EnvTable} from "../bareComponents/EnvTable";
import {Input} from "../bareComponents/Input";
import {PencilIcon} from "lucide-react";
import {useState} from "react";
import {EnvironmentWrapper} from "./EnvironmentType";

export const EnvironmentPanel = ()=>{
    const selectedEnvironment = useAPIStore(state=>state.selectedEnvironment)
    const setSelectedEnvironment = useAPIStore(state=>state.setSelectedEnvironment)
    const [inputDisabled, setInputDisabled] = useState<boolean>(true)
    const setSelectedEnvironments = useAPIStore(state=>state.setAllEnvs)
    const allEnv = useAPIStore(state=>state.allEnvs)

    return <div className="main-panel p-5 text-white">
            <h1 className="relative max-w-fit mb-10">
                <PencilIcon className="absolute -right-8 mt-2 w-8" onClick={()=>{
                    setInputDisabled(!inputDisabled)
                }}/>
                <Input onBlur={()=>{
                    setInputDisabled(true)
                    const mappedEnv = allEnv.map((a, index)=>{
                        if (index === selectedEnvironment?.index) {
                            return {
                                ...a,
                                name: selectedEnvironment.name
                            } satisfies EnvironmentWrapper
                        }
                        return a
                    })
                    setSelectedEnvironments(mappedEnv)
                }} className="max-w-fit" disabled={inputDisabled} value={selectedEnvironment?.name} onChange={(v)=>{
                setSelectedEnvironment({
                    ...selectedEnvironment!,
                    name: v.target.value,
                }, selectedEnvironment!.index)
            }} /></h1>
        <EnvTable/>
    </div>
}