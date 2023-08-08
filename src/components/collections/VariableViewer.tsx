import {useAPIStore} from "../../store/store";

export const VariableViewer = ()=>{
    const collection = useAPIStore(state=>state.currentCollection)

    return <table>
        <thead>
            <tr>
                <th>Key</th>
                <th>Value</th>
            </tr>
        </thead>
        {collection&& collection.variable?.map((v,i)=>{
        return <tr key={i}>
            <td>{v.key}</td>
            <td>{v.value}</td>
        </tr>
    })}
    </table>
}
