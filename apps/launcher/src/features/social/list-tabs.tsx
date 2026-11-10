// The list page while chat is on (CHAT1): "Arkadaşlar" and "Mesajlar" (with how many are unread),
// the friend list and the conversations; the panel opens on the one the player last picked, after a
// restart too (CREW-UI, `setListTab`). Without chat it is the friend list alone, as before.
// While the player's world is shared (HOST1) "Dünyan açık" sits above both. SOC-NEXT: the count
// takes the conversations marked unread here, and the tab's right click marks them all read.
import { TabPanel, Tabs } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { WorldStrip } from "@/features/host/world-strip";
import { ConversationList } from "./chat/conversation-list";
import { unreadAll } from "./chat/conversations";
import { ListPage } from "./list-page";
import { allReadItems } from "./next/conv-actions";
import { unreadShownAll } from "./next/conv-menu";
import { useSocNext } from "./next/flag";
import { useSocNextPrefs } from "./next/prefs";
import { type ListTab, useSocialUi } from "./store";

export function ListTabs() {
  return (
    <>
      <WorldStrip />
      <ListBody />
    </>
  );
}

function ListBody() {
  const { t } = useTranslation();
  const chat = useSocialUi((s) => s.view.chat);
  const tab = useSocialUi((s) => s.listTab);
  const setTab = useSocialUi((s) => s.setListTab);
  const next = useSocNext();
  const marks = useSocNextPrefs((s) => s.unreadMarks);
  if (!chat) return <ListPage />;
  const unread = next ? unreadShownAll(chat, marks) : unreadAll(chat);
  return (
    <Tabs<ListTab>
      value={tab}
      onValueChange={setTab}
      label={t("social.title")}
      items={[
        { value: "friends", label: t("chat.tabs.friends") },
        {
          value: "messages",
          label: t("chat.tabs.messages"),
          count: unread > 0 ? unread : undefined,
          menu: next ? allReadItems(chat, t) : undefined,
        },
      ]}
      className="-mt-1"
    >
      <TabPanel value="friends" className="overflow-visible pt-2">
        <ListPage />
      </TabPanel>
      <TabPanel value="messages" className="overflow-visible pt-2">
        <ConversationList />
      </TabPanel>
    </Tabs>
  );
}
