;;; kessler.el --- work kessler from emacs  -*- lexical-binding: t; -*-

;; part of kessler, GPL-3.0-or-later.

;;; Commentary:

;; play, pause and skip in kessler from emacs, and what plays in the mode
;; line. it talks to the running window the way kessler's own command line
;; does (`kessler toggle', `kessler now', ...), over the session bus — so on
;; linux, where kessler is reachable that way.
;;
;;   (load "/path/to/kessler.el")
;;   (kessler-keys-mode 1)        ; c-c k space, n, p, s, i
;;   (kessler-mode-line-mode 1)   ; what plays, in the mode line

;;; Code:

(defgroup kessler nil
  "Work the kessler music player from emacs."
  :group 'multimedia
  :prefix "kessler-")

(defcustom kessler-program "kessler"
  "The kessler program, by name on the path or as a full path."
  :type 'string)

(defcustom kessler-mode-line-every 5
  "How often, in seconds, the mode line asks what plays."
  :type 'integer)

(defun kessler--run (command)
  "Run kessler with COMMAND and return what it said, trimmed."
  (with-temp-buffer
    (let ((status (call-process kessler-program nil t nil command)))
      (let ((said (string-trim (buffer-string))))
        (unless (eq status 0)
          (user-error "%s" (if (string-empty-p said) "kessler did not answer" said)))
        said))))

(defun kessler-toggle ()
  "Play or pause."
  (interactive)
  (kessler--run "toggle"))

(defun kessler-next ()
  "The next track."
  (interactive)
  (kessler--run "next"))

(defun kessler-previous ()
  "The track before."
  (interactive)
  (kessler--run "prev"))

(defun kessler-stop ()
  "Stop."
  (interactive)
  (kessler--run "stop"))

(defun kessler-now ()
  "Say what plays."
  (interactive)
  (let ((said (kessler--run "now")))
    (message "%s" (if (string-empty-p said) "kessler: nothing plays" said))))

(defvar kessler-keys-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c k SPC") #'kessler-toggle)
    (define-key map (kbd "C-c k n") #'kessler-next)
    (define-key map (kbd "C-c k p") #'kessler-previous)
    (define-key map (kbd "C-c k s") #'kessler-stop)
    (define-key map (kbd "C-c k i") #'kessler-now)
    map)
  "Keys for kessler, under C-c k.")

;;;###autoload
(define-minor-mode kessler-keys-mode
  "Keys for kessler under C-c k: space, n, p, s and i."
  :global t
  :keymap kessler-keys-map)

(defvar kessler--mode-line "")
(defvar kessler--timer nil)

(defun kessler--ask ()
  "Ask what plays, without waiting for the answer."
  (let ((buffer (generate-new-buffer " *kessler now*")))
    (make-process
     :name "kessler-now"
     :buffer buffer
     :command (list kessler-program "now")
     :noquery t
     :connection-type 'pipe
     :stderr (make-pipe-process :name "kessler-now-err" :noquery t :buffer nil
                                :filter #'ignore :sentinel #'ignore)
     :sentinel
     (lambda (process _event)
       (unless (process-live-p process)
         (let ((said (with-current-buffer buffer (string-trim (buffer-string)))))
           (setq kessler--mode-line
                 (if (or (string-empty-p said) (not (eq (process-exit-status process) 0)))
                     ""
                   (concat " ♪ " said " ")))
           (kill-buffer buffer)
           (force-mode-line-update t)))))))

;;;###autoload
(define-minor-mode kessler-mode-line-mode
  "What kessler plays, in the mode line."
  :global t
  (when kessler--timer
    (cancel-timer kessler--timer)
    (setq kessler--timer nil))
  (setq global-mode-string (delete '(:eval kessler--mode-line) (or global-mode-string '(""))))
  (setq kessler--mode-line "")
  (when kessler-mode-line-mode
    (add-to-list 'global-mode-string '(:eval kessler--mode-line) t)
    (setq kessler--timer (run-at-time 0 kessler-mode-line-every #'kessler--ask))))

(provide 'kessler)
;;; kessler.el ends here
