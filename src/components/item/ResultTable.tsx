import {FC} from "react";

type ResultTableProps = {
    value: Object,
    emptyMessage: string
}

export const ResultTable:FC<ResultTableProps> = ({value, emptyMessage})=>{

    return <>{Object.keys(value).length === 0? <div>{emptyMessage}</div>:<table className="table-fixed w-full border-[1px] border-border_strong">
        <thead>
        <tr>
            <th>Key</th>
            <th>Value</th>
        </tr>
        </thead>
        <tbody>
        {
            Object.entries(value).map(([key, value])=>{
                return <tr className="p-2 border-[1px] border-border_strong">
                    <td className="p-2 border-[1px] border-border_strong">{key}</td>
                    <td className="p-2">{value}</td>
                </tr>
            })
        }
        </tbody>
    </table>
    }
</>
}
