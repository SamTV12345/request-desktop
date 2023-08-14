import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {useForm} from "react-hook-form";
import {VariableDefinition} from "postman-collection";

type APIKeyData = {
    key: string,
    value: string
    header: "header" | "query"
}

export const APiKeyAuthentication = ()=> {
    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        const filteredCollection =  currentCollection?.auth?.apikey?.filter((v)=>v.key === key)
        if(filteredCollection === undefined|| !filteredCollection[0]){
            return defaultValue
        }
        return filteredCollection[0].value
    }
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)

    const { register, handleSubmit
    } = useForm<APIKeyData>({
        defaultValues: {
            value: getKey('value', ""),
            key: getKey('key', ""),
            header: getKey('in', "header") as "header" | "query"
        }
    })


    const populateApiKeyAuth = (data: APIKeyData)=>{
        const newAuth = [{
            key: "key",
            type: "string",
            value: data.key

        },
            {
                key: "value",
                type: "string",
                value: data.value
            },
            {
                key: "in",
                type: "string",
                value: data.header
            }
        ] as VariableDefinition[]

        const clonedCollection:CollectionDefinitionExtended = {
            ...currentCollection!,
            auth: {
                ...currentCollection?.auth,
                type: "apikey",
                apikey: newAuth
            }
        }
        updateCollection(clonedCollection!)
        saveCollection()
    }

    return <form className="grid grid-cols-2 gap-5 text-white" onSubmit={handleSubmit(populateApiKeyAuth)}>
        <label>Key</label>
        <div><input className="bg-basecol p-1" {...register('key')}/></div>

        <div>Value</div>
        <div><input className="bg-basecol p-1" {...register('value')}/></div>

        <div>Add to</div>
        <select className="bg-basecol" {...register('header')}>
            <option value="header">Header</option>
            <option value="query">Query Params</option>
        </select>
        <button type="submit">Add</button>
    </form>
}
