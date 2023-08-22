import {ItemDefinitionExtended, useAPIStore} from "../../../store/store";
import {Editor} from "../Editor";
import {useDebounce} from "../../../hooks/useDebounce";

export const RawBody = () => {
    const item = useAPIStore(state => (state.currentItem as ItemDefinitionExtended))
    const setItem = useAPIStore(state => state.setCurrentItem)
    const changeBody = (v: string) => {
        const currentItemCloned: ItemDefinitionExtended = {
            ...item,
            request: {
                ...item.request!,
                body: {
                    ...item.request?.body!,
                    raw: v
                }
            }
        }
        setItem(currentItemCloned)
    }


    useDebounce(() => {

    }, 1000, [item.request!.body!.raw])

    return <Editor onChange={(v) => changeBody(v!)} value={item.request?.body?.raw!} readonly={false} mode="json"/>
}
