import * as React from 'react';
import {DataGrid, GridColDef, GridValueGetterParams} from '@mui/x-data-grid';
import {useAPIStore} from "../store/store";
import {ItemDefinition, QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";

const columns: GridColDef[] = [
    { field: 'id', headerName: 'Key',editable: true,width:200 },
    { field: 'value', headerName: 'Value',width:300, editable: true },
    { field: 'description', headerName: 'Description',width:800 },
];


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
        <div style={{ height: 400, width: '100%' }}>
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
        </div>
    );
}
