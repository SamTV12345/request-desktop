import {FC} from "react";
import {useNavigate} from "react-router-dom";

type NavBoxProps = {
    title?: string,
    logo?: React.ReactNode,
    to?: string
}


export const NavBox:FC<NavBoxProps> = ({title, logo, to})=>{
    const navigate = useNavigate()

    return <div className="w-full text-center" onClick={()=>{
        to && navigate(to)
    }}>
        <span  className="flex justify-center">{logo}</span>
        <span className="">{title}</span>
    </div>
}