using System;
using System.Drawing;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Diagnostics
{
    /// <summary>
    /// Başlatıcının tek hata bildirim penceresi. Premium girişteki "destek talebi aç" akışının
    /// genelleştirilmiş hâli: kullanıcıya sade bir açıklama, altında (istenirse açılan) teknik
    /// ayrıntı ve hatayı olduğu gibi paylaşmasını sağlayan kopyala/destek düğmeleri.
    /// </summary>
    internal sealed class ErrorReportForm : Form
    {
        private static readonly Color SurfaceColor = Color.FromArgb(26, 29, 35);
        private static readonly Color CardColor = Color.FromArgb(32, 36, 44);
        private static readonly Color BorderColor = Color.FromArgb(52, 58, 70);
        private static readonly Color TextPrimary = Color.FromArgb(245, 247, 250);
        private static readonly Color TextMuted = Color.FromArgb(152, 162, 179);
        private static readonly Color WarningSurface = Color.FromArgb(45, 38, 22);
        private static readonly Color WarningText = Color.FromArgb(226, 190, 132);
        private static readonly Color BrandColor = Color.FromArgb(248, 148, 35);
        private static readonly Color DangerColor = Color.FromArgb(239, 68, 68);

        private const int Gutter = 24;
        private const int CardWidth = 520;

        private readonly string _details;
        private readonly TextBox _detailsBox;
        private readonly Guna2Button _detailsToggle;
        private readonly Guna2Button _copyButton;

        private bool _detailsVisible;

        internal ErrorReportForm(string title, string message, string hint, string details)
        {
            _details = details ?? string.Empty;

            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.CenterParent;
            BackColor = SurfaceColor;
            ForeColor = TextPrimary;
            Font = new Font("Segoe UI", 9f);
            Text = string.IsNullOrEmpty(title) ? "Başlatıcı hatası" : title;
            ClientSize = new Size(CardWidth + Gutter * 2, 200);

            int y = Gutter;

            Label heading = new Label
            {
                Text = Text,
                Font = new Font("Segoe UI Semibold", 13f, FontStyle.Bold),
                ForeColor = DangerColor,
                AutoSize = false,
                Location = new Point(Gutter, y),
                Size = new Size(CardWidth, 26),
            };
            Controls.Add(heading);
            y = heading.Bottom + 10;

            Label body = MakeWrapLabel(message, TextMuted, y);
            Controls.Add(body);
            y = body.Bottom + 14;

            if (!string.IsNullOrEmpty(hint))
            {
                Guna2Panel hintPanel = new Guna2Panel
                {
                    FillColor = WarningSurface,
                    BorderColor = Color.FromArgb(92, 74, 40),
                    BorderThickness = 1,
                    BorderRadius = 8,
                    Location = new Point(Gutter, y),
                    Size = new Size(CardWidth, 10),
                };

                Label hintLabel = new Label
                {
                    Text = hint,
                    ForeColor = WarningText,
                    AutoSize = false,
                    Location = new Point(12, 10),
                    Size = new Size(CardWidth - 24, 10),
                };
                hintLabel.Size = new Size(
                    CardWidth - 24,
                    TextRenderer.MeasureText(hint, hintLabel.Font, new Size(CardWidth - 24, 0),
                        TextFormatFlags.WordBreak).Height);

                hintPanel.Size = new Size(CardWidth, hintLabel.Bottom + 10);
                hintPanel.Controls.Add(hintLabel);
                Controls.Add(hintPanel);
                y = hintPanel.Bottom + 14;
            }

            _detailsToggle = MakeGhostButton("Ayrıntıları göster", 160);
            _detailsToggle.Location = new Point(Gutter, y);
            _detailsToggle.Click += OnToggleDetailsClick;
            Controls.Add(_detailsToggle);
            y = _detailsToggle.Bottom + 10;

            _detailsBox = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                WordWrap = false,
                BackColor = CardColor,
                ForeColor = TextMuted,
                BorderStyle = BorderStyle.FixedSingle,
                Font = new Font("Consolas", 8.5f),
                Location = new Point(Gutter, y),
                Size = new Size(CardWidth, 220),
                Text = _details,
                Visible = false,
            };
            _detailsBox.GotFocus += (s, e) => _detailsBox.SelectionLength = 0;
            Controls.Add(_detailsBox);

            _copyButton = MakeGhostButton("Panoya kopyala", 150);
            _copyButton.Click += OnCopyClick;
            Controls.Add(_copyButton);

            Guna2Button logButton = MakeGhostButton("Kayıtları aç", 130);
            logButton.Click += (s, e) => LauncherDiagnostics.OpenLogFolder();
            Controls.Add(logButton);

            Guna2Button supportButton = MakeGhostButton("Destek talebi aç", 160);
            supportButton.FillColor = BrandColor;
            supportButton.ForeColor = Color.FromArgb(26, 29, 35);
            supportButton.BorderThickness = 0;
            supportButton.HoverState.FillColor = Color.FromArgb(255, 168, 62);
            supportButton.HoverState.ForeColor = Color.FromArgb(26, 29, 35);
            supportButton.Click += (s, e) => LauncherDiagnostics.OpenSupportPage();
            Controls.Add(supportButton);

            Guna2Button closeButton = MakeGhostButton("Kapat", 100);
            closeButton.Click += (s, e) => Close();
            Controls.Add(closeButton);

            _actionRow = new Guna2Button[] { _copyButton, logButton, supportButton, closeButton };

            LayoutActionRow();
            CancelButton = closeButton;
        }

        private readonly Guna2Button[] _actionRow;

        private Label MakeWrapLabel(string text, Color color, int top)
        {
            Label label = new Label
            {
                Text = text ?? string.Empty,
                ForeColor = color,
                AutoSize = false,
                Location = new Point(Gutter, top),
                Size = new Size(CardWidth, 10),
            };

            label.Size = new Size(
                CardWidth,
                TextRenderer.MeasureText(label.Text, label.Font, new Size(CardWidth, 0),
                    TextFormatFlags.WordBreak).Height + 2);

            return label;
        }

        private static Guna2Button MakeGhostButton(string text, int width)
        {
            return new Guna2Button
            {
                Text = text,
                Size = new Size(width, 36),
                BorderRadius = 8,
                BorderThickness = 1,
                BorderColor = Color.FromArgb(74, 81, 95),
                FillColor = Color.Transparent,
                ForeColor = TextPrimary,
                Font = new Font("Segoe UI", 9f),
                HoverState = { FillColor = Color.FromArgb(44, 49, 60) },
                Cursor = Cursors.Hand,
            };
        }

        private void OnToggleDetailsClick(object sender, EventArgs e)
        {
            _detailsVisible = !_detailsVisible;
            _detailsBox.Visible = _detailsVisible;
            _detailsToggle.Text = _detailsVisible ? "Ayrıntıları gizle" : "Ayrıntıları göster";
            LayoutActionRow();
        }

        private void OnCopyClick(object sender, EventArgs e)
        {
            try
            {
                if (string.IsNullOrEmpty(_details))
                    return;

                Clipboard.SetText(_details);
                _copyButton.Text = "Kopyalandı";
            }
            catch (Exception)
            {
                _copyButton.Text = "Kopyalanamadı";
            }
        }

        private void LayoutActionRow()
        {
            int top = _detailsVisible ? _detailsBox.Bottom + 16 : _detailsToggle.Bottom + 16;
            int x = Gutter;

            foreach (Guna2Button button in _actionRow)
            {
                button.Location = new Point(x, top);
                x = button.Right + 8;
            }

            // Son düğme (Kapat) sağa yaslanır; araya kalan boşluk ayırıcı görevi görür.
            Guna2Button last = _actionRow[_actionRow.Length - 1];
            last.Location = new Point(Gutter + CardWidth - last.Width, top);

            ClientSize = new Size(CardWidth + Gutter * 2, top + last.Height + Gutter);
        }
    }
}
