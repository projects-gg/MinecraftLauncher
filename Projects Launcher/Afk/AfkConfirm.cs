using System;
using System.Drawing;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Uygulama paletiyle uyumlu, sistem MessageBox'ının yerini tutan özel onay penceresi.
    /// Geri alınamaz eylemlerde (hesap silme, oturumu zorla durdurma vb.) kullanılır.
    /// </summary>
    public static class AfkConfirm
    {
        public static bool Ask(IWin32Window owner, string title, string message, string confirmText, bool danger)
        {
            using (Form form = new Form())
            {
                form.SuspendLayout();

                form.FormBorderStyle = FormBorderStyle.None;
                form.StartPosition = FormStartPosition.CenterParent;
                form.ShowInTaskbar = false;
                form.Size = new Size(420, 210);
                // Form'un kendisi kenarlık rengiyle boyanır; 1px içeride oturan panel kart yüzeyini
                // oluşturur. Böylece Guna2Elipse ile yuvarlatılan köşede ince bir çerçeve görünür.
                form.BackColor = Color.FromArgb(52, 58, 70);
                form.KeyPreview = true;

                Guna2Elipse formElipse = new Guna2Elipse();
                formElipse.TargetControl = form;
                formElipse.BorderRadius = 12;

                Panel content = new Panel();
                content.Location = new Point(1, 1);
                content.Size = new Size(form.ClientSize.Width - 2, form.ClientSize.Height - 2);
                content.BackColor = Color.FromArgb(32, 36, 44);
                form.Controls.Add(content);

                Label titleLabel = new Label();
                titleLabel.AutoSize = false;
                titleLabel.Location = new Point(20, 18);
                titleLabel.Size = new Size(content.Width - 40, 28);
                titleLabel.Font = new Font("Segoe UI", 12f, FontStyle.Bold);
                titleLabel.ForeColor = Color.FromArgb(245, 247, 250);
                titleLabel.Text = title ?? string.Empty;
                content.Controls.Add(titleLabel);

                // Pencere kendi başlık çubuğu olmadığı için başlık etiketinden sürüklenebilir.
                Guna2DragControl dragControl = new Guna2DragControl();
                dragControl.TargetControl = titleLabel;

                Label messageLabel = new Label();
                messageLabel.AutoSize = false;
                messageLabel.Location = new Point(20, 54);
                messageLabel.Size = new Size(content.Width - 40, 92);
                messageLabel.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
                messageLabel.ForeColor = Color.FromArgb(152, 162, 179);
                messageLabel.Text = message ?? string.Empty;
                content.Controls.Add(messageLabel);

                Guna2Button confirmButton = new Guna2Button();
                confirmButton.Text = confirmText;
                confirmButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
                confirmButton.ForeColor = Color.FromArgb(245, 247, 250);
                confirmButton.FillColor = danger ? Color.FromArgb(239, 68, 68) : Color.FromArgb(34, 197, 94);
                confirmButton.BackColor = content.BackColor; // yuvarlak köşeler kart yüzeyine karışsın
                confirmButton.BorderRadius = 8;
                confirmButton.Size = new Size(150, 36);
                confirmButton.Cursor = Cursors.Hand;
                confirmButton.Location = new Point(
                    content.Width - 20 - confirmButton.Width,
                    content.Height - 20 - confirmButton.Height);
                content.Controls.Add(confirmButton);

                Guna2Button cancelButton = new Guna2Button();
                cancelButton.Text = "Vazgeç";
                cancelButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
                cancelButton.ForeColor = Color.FromArgb(245, 247, 250);
                cancelButton.FillColor = Color.FromArgb(52, 58, 70);
                cancelButton.BackColor = content.BackColor; // yuvarlak köşeler kart yüzeyine karışsın
                cancelButton.BorderRadius = 8;
                cancelButton.Size = new Size(110, 36);
                cancelButton.Cursor = Cursors.Hand;
                cancelButton.Location = new Point(confirmButton.Left - 12 - cancelButton.Width, confirmButton.Top);
                content.Controls.Add(cancelButton);

                confirmButton.Click += delegate
                {
                    form.DialogResult = DialogResult.Yes;
                    form.Close();
                };

                cancelButton.Click += delegate
                {
                    form.DialogResult = DialogResult.No;
                    form.Close();
                };

                // Odak Guna2Button'lardan birinde olmayabileceği için Enter/Escape form düzeyinde
                // (KeyPreview) yakalanır; AcceptButton'a bağlamak odak durumuna bağımlı olurdu.
                form.KeyDown += delegate(object sender, KeyEventArgs e)
                {
                    if (e.KeyCode == Keys.Escape)
                    {
                        e.Handled = true;
                        form.DialogResult = DialogResult.No;
                        form.Close();
                    }
                    else if (e.KeyCode == Keys.Enter)
                    {
                        e.Handled = true;
                        form.DialogResult = DialogResult.Yes;
                        form.Close();
                    }
                };

                form.ResumeLayout(false);

                return form.ShowDialog(owner) == DialogResult.Yes;
            }
        }
    }
}
