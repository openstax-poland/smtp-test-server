import * as React from 'react'

import DateTime from '~/src/components/DateTime'
import MailboxList from '~/src/components/MailboxList'

import { Message, loadBody } from '~/src/data'

import './index.css'

interface Props {
    message: Message
}

export default function MailView({ message }: Props) {
    const [body, setBody] = React.useState<string>('')

    React.useEffect(() => {
        loadBody(message.id).then(setBody)
    }, [message.id, setBody])

    return <div className="mail-view">
        <div className="details">
            <Field name="From">
                <MailboxList mailboxes={message.from} />
            </Field>
            <Field name="Subject" value={message.subject} />
            <Field name="To">
                <MailboxList mailboxes={message.to} />
            </Field>
            <Field name="Sent">
                <DateTime format="medium" date={new Date(message.date * 1000)} />
            </Field>
        </div>
        <div className="body">
            <pre>{body}</pre>
        </div>
    </div>
}

interface FieldValueProps {
    name: string
    value: string
}

interface FieldChildrenProps {
    name: string
    children: React.ReactNode
}

function Field({ name, ...props }: FieldValueProps | FieldChildrenProps) {
    let children = 'children' in props ? props.children : <span>{props.value}</span>
    return <>
        <span className="field-name">{name}</span>
        {children}
    </>
}
