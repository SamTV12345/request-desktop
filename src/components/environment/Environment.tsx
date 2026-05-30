import ResizableBox from "../resizable/ResizableBox";
import {Input} from "../bareComponents/Input";
import {CheckCircle, EllipsisVertical, PlusIcon} from "lucide-react";
import {useEffect, useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {useAPIStore} from "../../store/store";
import {EnvironmentWrapper} from "./EnvironmentType";
import {EnvironmentPanel} from "./EnvironmentPanel";



const DEFAULT_ENV = {
    name: "New Environment",
    envVars: []
} satisfies EnvironmentWrapper

export const Environment = ()=>{
    const setEnvironments = useAPIStore(state=>state.setAllEnvs)
    const environments = useAPIStore(state=>state.allEnvs)
    const setSelectedEnvs = useAPIStore(state=>state.setSelectedEnvironment)

    useEffect(() => {
        invoke<EnvironmentWrapper[]>('get_environments')
            .then((c)=>setEnvironments(c))
    }, []);


    return <><ResizableBox direction={"right"} initialSize={200} className={"sidebar"}>
        <span className="flex gap-2 mb-2"><PlusIcon className="self-center active:scale-95 cursor-pointer" onClick={()=>{
            setEnvironments([...environments,DEFAULT_ENV])
        }}/><Input/></span>
        <hr/>
        <ol className="mt-5  w-full">
            <li className="flex w-full group/list-item cursor-pointer">
                <span>Globals</span>
                <span className="grow"></span>
                <span className="flex group-hover/list-item:visible invisible">
                    <CheckCircle className=""/>
                    <EllipsisVertical/>
                </span>
                <hr/>
            </li>
            {
                environments.map((env, i)=>{
                    return <li key={env.name} onClick={()=>{
                        setSelectedEnvs(env, i)
                    }}>
                        {env.name}
                    </li>
                })
            }
        </ol>
    </ResizableBox>
        <EnvironmentPanel/>
    </>
    }