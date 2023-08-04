import * as React from 'react';
import {useAPIStore} from "../../store/store";
import {ItemDefinition, QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";


export default function DataTable() {
    const currentItem = useAPIStore(state=>state.currentItem)

    const getRowInformation = (currentItem: ItemDefinition)=>{
        if (currentItem.request&& currentItem.request.url){
            const urls = (currentItem?.request.url as UrlDefinition).query as QueryParamDefinition[]
            return urls.map(v=>{
                return {id: v.key, value: v.value, description: v.description, disabled: v.disabled}
            })
        }
    }

    const mapped_items = useMemo(()=>{
        if(currentItem){
            return getRowInformation(currentItem)
        }
        return []
    }, [currentItem])



    return (
            <table className="query-param-table">
                <thead>
                <tr>
                    <th></th>
                    <th>Key</th>
                    <th>Value</th>
                    <th>Description</th>
                </tr>
                </thead>
                <tbody>
                {mapped_items&&mapped_items.map((v,i)=>{
                    return <tr key={i}>
                        <td>
                            <input type="checkbox" checked={!v.disabled}/>
                        </td>
                        <td>{v.id}</td>
                        <td>{v.value}</td>
                        <td className="description-table-cell">{v.description as string}</td>
                    </tr>
                })}
                </tbody>
            </table>
    );
}
