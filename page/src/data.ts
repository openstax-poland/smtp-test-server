/** Single email message */
export interface Message {
    /** Message ID */
    id: string
    /** Subject */
    subject: string
    /** Sender's email address */
    from: string
    /** Addressee's email address */
    to: string
    /** Date and time when this message was sent */
    date: string,
}
