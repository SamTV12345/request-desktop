import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {APIRequestSidebarIcon} from "./APIRequest";
import {SidebarContextMenu} from "../bareComponents/SidebarContextMenu";
import {ItemContextMenu} from "./ItemContextMenu";
import {FC, useEffect, useMemo} from "react";
import {ItemDefinition} from "postman-collection";

type ItemSidebarComponentProps = {
    item: ItemDefinition,
    index: number,
    collection:CollectionDefinitionExtended
}

export const ItemSidebarComponent:FC<ItemSidebarComponentProps> = ({item,index, collection})=>{
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const isVisible = useMemo(()=>{
        return collection?.type === DisplayType.SINGLE_TYPE && currentItem?.id === item.id
    }, [item])
    const metadata = useAPIStore(state=>state.metadata)

    const openItem = ()=>{
            if(currentItem) {
                const valInMap = metadata.get(currentItem.id!)
                if(valInMap){
                    metadata.set(currentItem.id!, {...valInMap, selected: false, id: item.id!})
                }
            }
            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
            setCurrentCollection({...collection!, type: DisplayType.SINGLE_TYPE})
            const valInMap = metadata.get(item.id!)
            if(valInMap){
                metadata.set(item.id!, {...valInMap, selected: true, id: item.id!})
            }
            else{
                metadata.set(item.id!, {selected: true, id: item.id!})
            }
    }

    useEffect(() => {
        if(metadata.has(item.id!)){
            const valInMap = metadata.get(item.id!)
            if(valInMap?.selected){
                setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
                setCurrentCollection({...collection!, type: DisplayType.SINGLE_TYPE})
            }
        }
    }, []);

    const Label = ()=><span onClick={openItem}>{item.name!}</span>
    return <div key={index} style={{marginLeft: `2em`}}
         onClick={openItem} className={`sidebar-request border-none ${isVisible && 'bg-background_tertiary'}`}>
        <APIRequestSidebarIcon type={item.request?.method as string}/>
        <SidebarContextMenu triggerLabel={<Label/>} children={<ItemContextMenu/>}/>
    </div>
}
