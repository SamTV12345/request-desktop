import {Tabs, TabsContent, TabsList, TabsTrigger} from "@radix-ui/react-tabs";
import {useAPIStore} from "../../../store/store";
import {BodyResponseEditor} from "../BodyResponseEditor";
import {CookieResponseViewer} from "../CookieResponseViewer";
import {HeaderResponseViewer} from "../HeaderResponseViewer";
import {useEffect} from "react";

export const ResponseBar = ()=>{
    const currentRequest = useAPIStore(state => state.currentRequest)
   const responseExtended = useAPIStore(state => state.responseExtended)
    const setResponseExtended = useAPIStore(state => state.setResponseExtended)
    const getByteSize = (body:string) => new Blob([body]).size;

    useEffect(() => {
        if(currentRequest){
            setResponseExtended(true)
        }
    }, [currentRequest]);
    const convertToReadableUnits = (bytes:number) => {
        const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB']
        if (bytes === 0) {
            return 'n/a'
        }
        const i = Math.floor(Math.log(bytes) / Math.log(1024))
        if (i === 0) {
            return `${bytes} ${sizes[i]})`
        }
        return `${(bytes / (1024 ** i)).toFixed(2)} ${sizes[i]}`
    }

    return <>
    {!currentRequest&&<div className="text-tertiary flex">
            <span className="mt-5">Response</span>
            <span className="flex-grow"></span>
            <button onClick={()=>setResponseExtended(!responseExtended)} className={`${responseExtended&&'rotate-180'} material-symbols-outlined`}>expand_less</button>
        </div>
    }
        {
            !currentRequest && !responseExtended &&<div className="grid place-items-center h-full text-white">No response</div>
        }
        {currentRequest&&
        <Tabs defaultValue="body" className="selector text-white p-2 tab-container">
        <TabsList className="query-param-list">
            <TabsTrigger value="body" className="">Body</TabsTrigger>
            <TabsTrigger value="cookies">Cookies</TabsTrigger>
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="test-results">Test Results</TabsTrigger>
            <div className="flex-grow"></div>
            <div>Status</div>
            <div className="text-green-400">{currentRequest?.status}</div>
            <div>Time</div>
            <div className="text-green-400">{currentRequest?.duration.response_duration} ms</div>
            <div>Size</div>
            <div className="text-green-400">{convertToReadableUnits(getByteSize(currentRequest?.body!))}</div>
        </TabsList>
        <TabsContent value="body" className={"response-body-viewer"}><BodyResponseEditor/></TabsContent>
        <TabsContent value="cookies"><CookieResponseViewer/></TabsContent>
        <TabsContent value="headers"><HeaderResponseViewer/></TabsContent>
        <TabsContent value="test-results"><div/></TabsContent>
    </Tabs>
        }
        </>
}
