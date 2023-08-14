import {useForm} from "react-hook-form";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {VariableDefinition} from "postman-collection";


type BearerData = {
    token:string
}

export const BearerAuthentication = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        const filteredCollection =  currentCollection?.auth?.bearer?.filter((v)=>v.key === key)
        if(filteredCollection === undefined|| !filteredCollection[0]){
            return defaultValue
        }
        return filteredCollection[0].value
    }


    const { register, handleSubmit
    } = useForm<BearerData>({
        defaultValues: {
            token: getKey('token', "")
        }
    });

    const populateBearerKeyAuth = (data: BearerData)=>{
        const newAuth = [{
            key: "token",
            type: "string",
            value: data.token
        }
        ] as VariableDefinition[]

        const clonedCollection:CollectionDefinitionExtended = {
            ...currentCollection!,
            auth: {
                ...currentCollection?.auth,
                type: "bearer",
                bearer: newAuth
            }
        }
        updateCollection(clonedCollection!)
        saveCollection()
    }


    return <form onSubmit={handleSubmit(populateBearerKeyAuth)} className="grid grid-cols-2 gap-5 text-white">
        <label>Token:</label>
        <input {...register('token')} className="bg-basecol p-1"/>
        <button type="submit">Speichern</button>
    </form>
}
