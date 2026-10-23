// The list page while chat is on (CHAT1): "Arkadaşlar" and "Mesajlar" (with how many are unread),
// the friend list and the conversations. Without chat it is the friend list alone, as before.
// While the player's world is shared (HOST1) "Dünyan açık" sits above both.
import { TabPanel, Tabs } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { WorldStrip } from "@/features/host/world-strip";
import { ConversationList } from "./chat/conversation-list";
import { unreadAll } from "./chat/conversations";
import { ListPage } from "./list-page";
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
  if (!chat) return <ListPage />;
  const unread = unreadAll(chat);
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
