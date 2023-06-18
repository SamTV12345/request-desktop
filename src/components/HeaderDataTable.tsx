import * as React from 'react';
import {DataGrid, GridColDef, GridValueGetterParams} from '@mui/x-data-grid';
import {useAPIStore} from "../store/store";
import {QueryParamDefinition, UrlDefinition} from "postman-collection";

const columns: GridColDef[] = [
    { field: 'id', headerName: 'Key',editable: true,width:200 },
    { field: 'value', headerName: 'Value',width:300, editable: true },
    { field: 'description', headerName: 'Description',width:800 },
];


export const HeaderDataTable = ()=> {
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const getRowInformation = ()=>{
        if (currentItem?.request&& currentItem.request.header){
            const headers = currentItem.request.header

            return headers&&headers.map(v=>{
                return {id: v.key, value: v.value, description: v.description}
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
            <DataGrid
                rows={getRowInformation()}
                columns={columns}
                initialState={{
                    pagination: {
                        paginationModel: { page: 0, pageSize: 5 },
                    },
                }}
                rowSelectionModel={getCheckedRows()}
                onRowEditCommit={(params, event,p)=>{
                    return console.log(params);
                }
                }
                onRowSelectionModelChange={(rowSelectionModel, details)=>{
                    ((currentItem?.request!.url as UrlDefinition).query as QueryParamDefinition[]).forEach(v=>{

                        let resultingQuery = (currentItem?.request?.header.map(v=>{
                            if (!rowSelectionModel.includes(v.key!)){
                                v.disabled=true
                                return v
                            }
                            else{
                                v.disabled=false
                                return v
                            }
                        }))

                        if (resultingQuery && currentItem?.request){
                            setCurrentItem({...currentItem, request: {...currentItem.request, header: [...resultingQuery]}})
                        }
                    })}}
                pageSizeOptions={[5, 10]}
                checkboxSelection
            />
        </div>
    );
}
