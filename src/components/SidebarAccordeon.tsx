import {FC} from "react";
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
} from "./Accordeon"
import {useAPIStore} from "../store/store";
import {APIRequestSidebarIcon} from "./APIRequest";


type SidebarAccordeonProps = {
    collection: CollectionDefinition
}

export const isItemGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return 'item' in item && item.item != undefined
}

export const SidebarAccordeon:FC<SidebarAccordeonProps> = ({collection}) => {
    const initialNumber = 1

        return  <Accordion type="single" collapsible  key={collection.name+"name"}>
            <AccordionItem value="item-1" key={collection.name+"item"}>
                <AccordionTrigger>{collection.info?.name}</AccordionTrigger>
                        <AccordionContent className="">
                            <RecursiveItemGroup item={collection.item} indent={initialNumber}/>
                        </AccordionContent>
            </AccordionItem>
    </Accordion>
}

type RecursiveItemGroupProps = {
    item: (ItemGroupDefinition | ItemDefinition)[] | undefined,
    indent: number
}
export const RecursiveItemGroup:FC<RecursiveItemGroupProps> = ({item,indent})=>{
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)

    return <div>{item?.map((item, i)=>{
            if(isItemGroupDefinition(item)){
                return <Accordion type="single" collapsible style={{margin: `${indent*6}%`}} key={item.name}>
                    <AccordionItem value="item-1">
                        <AccordionTrigger  onClick={()=>{setCurrentItem(item)}}>{item.name}</AccordionTrigger>
                        <AccordionContent className="recursive-item">
                            <RecursiveItemGroup key={item.id} item={item.item} indent={indent+1}/>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
            }
            else{
                return <div key={i} style={{margin: `${indent*3}%`}} onClick={()=>{setCurrentItem(item)}} className="sidebar-request">
                    <APIRequestSidebarIcon type={item.request?.method as string}/>
                    {item.name}
                </div>
            }
        })}</div>

}


