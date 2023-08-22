import {Tabs, TabsContent, TabsList, TabsTrigger} from "@radix-ui/react-tabs";
import {useAPIStore} from "../../../store/store";
import {BodyResponseEditor} from "../BodyResponseEditor";
import {CookieResponseViewer} from "../CookieResponseViewer";
import {HeaderResponseViewer} from "../HeaderResponseViewer";

export const ResponseBar = ()=>{
    const currentRequest = useAPIStore(state => state.currentRequest)

    const getByteSize = (body:string) => new Blob([body]).size;

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


    return <Tabs defaultValue="body" className="selector text-white p-2 tab-container">
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
