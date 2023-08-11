import {useAPIStore} from "../../store/store";
import {Collection, VariableDefinition} from "postman-collection";
import {EditableTable} from "../bareComponents/EditableTable";

export const VariableViewer = ()=>{
    const collection = useAPIStore(state=>state.currentCollection)
    const setVariable = useAPIStore(state=>state.changeVarInCollection)
    const addVariable = useAPIStore(state=>state.addVarInCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)

    const disableVariable = (collectionId: string, disabled: boolean, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            disabled: disabled
        }
        setVariable(collection!.id!,newVariable, index)
    }

    const onKeyChange = (collectionId: string, newKey: string, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            key: newKey
        }
        setVariable(collection!.id!,newVariable, index)
    }

    const onValueChange = (collectionId: string, newVal: string, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            value: newVal
        }
        setVariable(collectionId,newVariable, index)
    }

    const onAdd = (collectionId: string)=>{
        addVariable(collectionId)
    }

    const onSaveOfVariable = ()=>{
        saveCollection()
    }

    const onDelete = (collectionId: string, index:number)=>{
        setVariable(collectionId,undefined, index)
    }

    return <EditableTable value={collection?.variable!} onDisabled={disableVariable} onKeyChange={onKeyChange}
                          onValueChange={onValueChange} onAdd={onAdd} onSave={onSaveOfVariable} onDelete={onDelete}/>
}
