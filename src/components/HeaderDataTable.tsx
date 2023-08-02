import * as React from 'react';
import {useAPIStore} from "../store/store";



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
