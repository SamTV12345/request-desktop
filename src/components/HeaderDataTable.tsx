import * as React from 'react';
import {DataGrid, GridColDef, GridValueGetterParams} from '@mui/x-data-grid';
import {useAPIStore} from "../store/store";

const columns: GridColDef[] = [
    { field: 'id', headerName: 'Key',editable: true,width:200 },
    { field: 'value', headerName: 'Value',width:300, editable: true },
    { field: 'description', headerName: 'Description',width:800 },
];


export const HeaderDataTable = ()=> {
    const currentItem = useAPIStore(state=>state.currentItem)
    const getRowInformation = ()=>{
        if (currentItem?.request&& currentItem.request.header){
            const headers = currentItem.request.header

            return headers&&headers.map(v=>{
                return v
            })
        }
    }


    const getCheckedRows = ()=>{
        if (currentItem?.request&& currentItem.request.header){
            const urls = currentItem?.request.header
            return urls.filter(c=>!c.disabled).map(v=>{
                return v.key
            })
        }
    }
    return (
        <div style={{ height: 400, width: '100%' }}>
        <table>
            <thead>
            <tr>
                <th>Key</th>
                <th>Value</th>
                <th>Description</th>
            </tr>
            </thead>
            <tbody>
            {currentItem?.request&& currentItem.request.header!.map((v,i)=>{
                return <tr key={i}>
                    <td>
                        <input type="checkbox" checked={!v.disabled}/>
                    </td>
                    <td>{v.id}</td>
                    <td>{v.value}</td>
                    <td>{v.description as string}</td>
                </tr>
            })}
            </tbody>
        </table>
        </div>
    )
}
