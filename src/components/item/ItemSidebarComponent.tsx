import {DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {APIRequestSidebarIcon} from "./APIRequest";
import {SidebarContextMenu} from "../bareComponents/SidebarContextMenu";
import {ItemContextMenu} from "./ItemContextMenu";
import {FC, useMemo} from "react";
import {ItemDefinition} from "postman-collection";

type ItemSidebarComponentProps = {
    item: ItemDefinition,
    index: number
}

export const ItemSidebarComponent:FC<ItemSidebarComponentProps> = ({item,index})=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const isVisible = useMemo(()=>{
        return currentCollection?.type === DisplayType.SINGLE_TYPE && currentItem?.id === item.id
    }, [item])

    const openItem = ()=>{
            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
            setCurrentCollection({...currentCollection!, type: DisplayType.SINGLE_TYPE})
    }

    const Label = ()=><span onClick={openItem}>{item.name!}</span>
    return <div key={index} style={{marginLeft: `2em`}}
         onClick={openItem} className={`sidebar-request border-none ${isVisible && 'bg-background_tertiary'}`}>
        <APIRequestSidebarIcon type={item.request?.method as string}/>
        <SidebarContextMenu triggerLabel={<Label/>} children={<ItemContextMenu/>}/>
    </div>
}
