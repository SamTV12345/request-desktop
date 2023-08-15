import {useForm} from "react-hook-form";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";


type BasicAuthenticationData = {
    username: string,
    password: string
}

export const BasicAuthentication = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        const filteredCollection =  currentCollection?.auth?.basic?.filter((v)=>v.key === key)
        if(filteredCollection === undefined|| !filteredCollection[0]){
            return defaultValue
        }
        return filteredCollection[0].value
    }

    const { register, handleSubmit} = useForm<BasicAuthenticationData>({
        defaultValues: {
            password: getKey('password', ""),
            username: getKey('username', "")
        }
    })

    const populateBasicAuth = (data: BasicAuthenticationData)=>{
        const newAuth = [{
            key: "username",
            type: "string",
            value: data.username

        },
            {
                key: "password",
                type: "string",
                value: data.password
            }
        ]

        const clonedCollection:CollectionDefinitionExtended = {
            ...currentCollection!,
            auth: {
                ...currentCollection?.auth,
                type: "basic",
                basic: newAuth
            }
        }
        updateCollection(clonedCollection)
        saveCollection()

    }

    return <form className="grid grid-cols-2 gap-5 text-white" onSubmit={handleSubmit(populateBasicAuth)}>
        <label>Username:</label>
        <input {...register('username')} className="bg-basecol p-1"/>
        <label>Password:</label>
        <input {...register('password')} className="bg-basecol p-1"/>
        <button type="submit">Save</button>
    </form>
}
