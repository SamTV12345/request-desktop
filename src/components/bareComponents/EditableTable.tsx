import {useAPIStore} from "../../store/store";
import {FC} from "react";
import {QueryParamDefinition, VariableDefinition} from "postman-collection";


type EditableTableProps<T> = {
    value: T[],
    onDisabled:(collectionId: string, disabled: boolean, index: number, itemId?:string)=>void,
    onKeyChange:(collectionId: string, newKey: string, index: number, itemId?:string)=>void,
    onValueChange:(collectionId: string, newVal: string, index: number, itemId?:string)=>void,
    onDescriptionChange: (collectionId: string, newVal: string, index: number, itemId?:string)=>void,
    onAdd: (collectionId: string)=>void,
    onSave: ()=>void,
    onDelete: (collectionId: string, index: number)=>void
}

export const EditableTable:FC<EditableTableProps<VariableDefinition|QueryParamDefinition>> = ({value,
                                                                             onValueChange,onKeyChange,onDisabled,
                                                                             onAdd, onSave, onDelete, onDescriptionChange})=>{
     const collection = useAPIStore(state=>state.currentCollection)

        if(!collection){
            return <div></div>
        }
        return <>
            <table className="table-fixed w-full">
                <thead>
                <tr>
                    <th></th>
                    <th>Key</th>
                    <th>Value</th>
                    <th>Description</th>
                    <th>Action</th>
                </tr>
                </thead>
                {collection&&value.map((v,i)=>{
                    return <tr key={i}>
                        <td><input type="checkbox" checked={v.disabled !== undefined ?!v.disabled: true} onChange={v=>{
                            onDisabled(collection.id!,!v.target.checked,i)
                        }}/></td>
                        <td><input className="bg-transparent border-2 border-gray-500 p-1" value={v.key!} onChange={(val)=>{
                            onKeyChange(collection?.id!,val.target.value,i)
                        }}/></td>
                        <td><input className="bg-transparent border-2 border-gray-500 p-1" value={v.value} onChange={(val)=>
                            onValueChange(collection?.id!,val.target.value,i)}/>
                        </td>
                        <td>
                            <input className="bg-transparent border-2 border-gray-500 p-1" value={v.description as string} onChange={(val)=>
                                onDescriptionChange(collection?.id!,val.target.value, i)}/>
                        </td>
                        <td>
                            <button className="bg-red-500 p-1 rounded" onClick={()=>onDelete(collection.id!, i)}>Löschen</button>
                        </td>
                    </tr>
                })}
            </table>
            <div className="float-right flex flex-col gap-5">
                <button className="bg-mustard-600 p-2 rounded float-right flex" onClick={()=>onAdd(collection.id!)}>
                    <span className="material-symbols-outlined self-center">add</span>Hinzufügen</button>
                <button className="bg-mustard-600 p-2 rounded float-right flex" onClick={()=>onSave()}>
                    <span className="material-symbols-outlined self-center">save</span>
                    Speichern</button>
            </div>
        </>

}
