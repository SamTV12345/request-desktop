import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {useState} from "react";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {CollectionDefinition, Url as URLParser} from "postman-collection";
import {useDebounce} from "../../hooks/useDebounce";

export const ItemNameAdapter = ()=>{
    const currentItem = useAPIStore(state => state.currentItem)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const [readonly,setReadonly] = useState<boolean>(true)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const setCurrentItem = useAPIStore(state => state.setCurrentItem)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const changeName = (name: string) => {
        const item:ItemDefinitionExtended  = {
            ...currentItem,
            name,
            type: DisplayType.SINGLE_TYPE
        }

        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} satisfies CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
        setCurrentItem(item)
    }

    useDebounce(()=>{
        saveCollection()
    },5000,[currentItem?.name])

    return <div className="request-name-section text-white mb-2  w-2/5 relative p-2">
        <input readOnly={readonly} value={currentItem?.name} className="text-white bg-basecol w-full p-2 read-only:text-gray-500 pr-10"
               onChange={(v)=>{changeName(v.target.value)}} onBlur={()=>setReadonly(true)}/>
        <span className="material-symbols-outlined absolute right-2 text-sm p-2" onClick={()=>setReadonly(!readonly)}>edit</span>
    </div>
}
