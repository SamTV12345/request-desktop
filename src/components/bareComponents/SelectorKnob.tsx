import {FC} from "react";

type SelectorKnobProps = {
    value: boolean
    onChange: (checked:boolean)=>void,
    label: string
}

export const SelectorKnob:FC<SelectorKnobProps>= ({onChange,value,label})=>{
    return <div className="flex gap-5">
        <input type="checkbox" checked={value} onChange={v=>onChange(v.target.checked)}/>
        {label}
    </div>
}
