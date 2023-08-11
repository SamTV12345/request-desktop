import {useAPIStore} from "../../store/store";

export const VariableViewer = ()=>{
    const collection = useAPIStore(state=>state.currentCollection)
    const setVariable = useAPIStore(state=>state.changeVarInCollection)
    const addVariable = useAPIStore(state=>state.addVarInCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)

    return <>
            <table className="table-fixed w-full">
            <thead>
                <tr>
                    <th>Key</th>
                    <th>Value</th>
                    <th>Action</th>
                </tr>
            </thead>
            {collection && collection.variable&& collection.variable.map((v,i)=>{
            return <tr key={i}>
                <td><input className="bg-transparent border-2 border-gray-500 p-1" value={v.key} onChange={(val)=>{
                    const newVar = {
                        ...v,
                        key: val.target.value
                    }

                    setVariable(collection.id!,newVar, i)
                }}/></td>
                <td><input className="bg-transparent border-2 border-gray-500 p-1" value={v.value} onChange={(val)=>{
                    const newVar = {
                        ...v,
                        value: val.target.value
                    }

                    setVariable(collection.id!,newVar, i)
                }}/></td>
                <td>
                    <button className="bg-red-500 p-1 rounded" onClick={()=>{
                        setVariable(collection.id!,undefined, i)
                    }}>Löschen</button>
                </td>
            </tr>
        })}
        </table>
        <div className="float-right flex flex-col gap-5">
            <button className="bg-mustard-600 p-2 rounded float-right" onClick={()=>addVariable(collection?.id!)}>Hinzufügen</button>
            <button className="bg-mustard-600 p-2 rounded float-right" onClick={()=>saveCollection()}>Speichern</button>
        </div>

    </>
}
