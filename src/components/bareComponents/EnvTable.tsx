import {useAPIStore} from "../../store/store";
import {EnvironmentType, EnvironmentWrapper} from "../environment/EnvironmentType";
import {invoke} from "@tauri-apps/api/core";

export const EnvTable = ()=>{
    const env_table = useAPIStore(state=>state.selectedEnvironment)
    const setEnv = useAPIStore(state=>state.setSelectedEnvironment)

    const onKeyChange = (oldId: string, newId: string, index: number)=>{
        const mappedEntries = env_table!.envVars.map((e,i)=>{
            if (i === index) {
                return {
                    ...e,
                     id: newId
                } satisfies EnvironmentType
            }
            return e
        })

        setEnv({
            ...env_table!,
            envVars: mappedEntries
        } satisfies EnvironmentWrapper,env_table!.index)
    }

    const onTypeChange = (oldId: string, newId: string, index: number)=>{
        const mappedEntries = env_table!.envVars.map((e,i)=>{
            if (i === index) {
                return {
                    ...e,
                    type: newId
                } satisfies EnvironmentType
            }
            return e
        })

        setEnv({
            ...env_table!,
            envVars: mappedEntries,
        } satisfies EnvironmentWrapper, env_table!.index)
    }

    const onInitialValueChange = (oldId: string, newId: string, index: number)=>{
        const mappedEntries = env_table!.envVars.map((e,i)=>{
            if (i === index) {
                return {
                    ...e,
                    initialValue: newId
                } satisfies EnvironmentType
            }
            return e
        })

        setEnv({
            ...env_table!,
            envVars: mappedEntries
        } satisfies EnvironmentWrapper, env_table!.index)
    }

    const onCurrentValChange = (oldId: string, newId: string, index: number)=>{
        const mappedEntries = env_table!.envVars.map((e,i)=>{
            if (i === index) {
                return {
                    ...e,
                    currentValue: newId
                } satisfies EnvironmentType
            }
            return e
        })

        setEnv({
            ...env_table!,
            envVars: mappedEntries
        } satisfies EnvironmentWrapper, env_table!.index)
    }

    const onAdd = (id: string)=>{
        const mappedArr = [...env_table!.envVars!,{
            id: '',
            type: 'Default',
            currentValue: '',
            initialValue: '',
        }] satisfies EnvironmentType[]

        setEnv({
            ...env_table!,
            envVars: mappedArr
        }, env_table!.index)
    }

    const onSave = ()=>{
        console.log('e',env_table)

        invoke('update_environments', {
            envWrapper: env_table
        })
    }

    return <>
        <table className="table-fixed w-full mb-5">
            <thead>
            <tr>
                <th>Variable</th>
                <th>Type</th>
                <th>Initial Value</th>
                <th>Current Value</th>
            </tr>
            </thead>
            <tbody>
            {env_table&&env_table.envVars.map((v,i)=>{
                return <tr key={i}>
                    <td className="text-center"><input className="bg-transparent border-2 border-gray-500 p-1" value={v.id!}
                               onChange={(val) => {
                                   onKeyChange(v?.id!, val.target.value, i)
                               }}/></td>
                    <td className="text-center"><input className="bg-transparent border-2 border-gray-500 p-1" value={v.type} onChange={(val) =>
                        onTypeChange(v?.id!, val.target.value, i)}/>
                    </td>
                    <td className="text-center">
                        <input className="bg-transparent border-2 border-gray-500 p-1" value={v.initialValue}
                               onChange={(val) =>
                                   onInitialValueChange(v?.id!, val.target.value, i)}/>
                    </td>
                    <td className="text-center">
                        <input className="bg-transparent border-2 border-gray-500 p-1" value={v.currentValue}
                               onChange={(val) =>
                                   onCurrentValChange(v?.id!, val.target.value, i)}/>
                    </td>
                </tr>
            })}
            </tbody>
        </table>
        <div className="float-right flex flex-col gap-5">
            <button className="bg-mustard-600 p-2 rounded-sm float-right flex" onClick={() => onAdd(env_table!.name!)}>
                <span className="material-symbols-outlined self-center">add</span>Hinzufügen</button>
            <button className="bg-mustard-600 p-2 rounded-sm float-right flex" onClick={()=>onSave()}>
                <span className="material-symbols-outlined self-center">save</span>
                Speichern</button>
        </div>
    </>
}