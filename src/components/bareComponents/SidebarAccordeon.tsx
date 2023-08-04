import {FC} from "react";
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {Accordion, AccordionContent, AccordionItem, AccordionTrigger,} from "./Accordeon"
import {DisplayType, useAPIStore} from "../../store/store";
import {APIRequestSidebarIcon} from "../item/APIRequest";


type SidebarAccordeonProps = {
    collection: CollectionDefinition
}

export const isItemGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return 'item' in item && item.item != undefined
}

export const SidebarAccordeon:FC<SidebarAccordeonProps> = ({collection}) => {
    const initialNumber = 1

    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)

        return  <Accordion type="single" collapsible  key={collection.name+"name"}>
            <AccordionItem value="item-1" key={collection.name+"item"}>
                    <AccordionTrigger><span onClick={()=>{
                        setCurrentCollection({...collection, type: DisplayType.COLLECTION_TYPE})
                        setCurrentItem(undefined)
                    }}>{collection.info?.name}</span></AccordionTrigger>
                        <AccordionContent className="">
                            <RecursiveItemGroup item={collection.item} indent={initialNumber} collection={collection}/>
                        </AccordionContent>
            </AccordionItem>
    </Accordion>
}

type RecursiveItemGroupProps = {
    item: (ItemGroupDefinition | ItemDefinition)[] | undefined,
    indent: number,
    collection: CollectionDefinition
}
export const RecursiveItemGroup:FC<RecursiveItemGroupProps> = ({item,indent, collection})=>{
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)

    return <div>{item?.map((item, i)=>{
            if(isItemGroupDefinition(item)){
                return <Accordion type="single" collapsible style={{marginLeft: `${indent*6}%`}} key={item.name}>
                    <AccordionItem value="item-1">
                        <AccordionTrigger  onClick={()=>{
                            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
                            setCurrentCollection({...collection, type: DisplayType.SINGLE_TYPE})
                        }}>{item.name}</AccordionTrigger>
                        <AccordionContent className="recursive-item">
                            <RecursiveItemGroup key={item.id} item={item.item} indent={indent+1} collection={collection}/>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
            }
            else{
                return <div key={i} style={{marginLeft: `${indent*3}%`}} onClick={()=>setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})} className="sidebar-request">
                    <APIRequestSidebarIcon type={item.request?.method as string}/>
                    {item.name}
                </div>
            }
        })}</div>

}


