use std::path::PathBuf;

use super::ports::{MergeOutcome, PortError};
use super::ship::ShipError;
use crate::domain::{AccountId, DomainError, Provider, TaskId};
use crate::tr;

/// Why a use case failed. Messages are user-facing, in the current language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("{}", self.message())]
    AccountNotFound(String),
    #[error("{}", self.message())]
    AccountExists(AccountId),
    #[error("{}", self.message())]
    DefaultFolderTaken(&'static str),
    #[error("{}", self.message())]
    AccountInUse {
        account: AccountId,
        projects: Vec<PathBuf>,
    },
    #[error("{}", self.message())]
    NoAccountResolved,
    #[error("{}", self.message())]
    NotASubscription(AccountId),
    #[error("{}", self.message())]
    NotAnApiAccount(AccountId),
    #[error("{}", self.message())]
    EmptyApiKey,
    #[error("{}", self.message())]
    DefaultCannotBeProtected(AccountId),
    #[error("{}", self.message())]
    InvalidSettingValue { key: String, value: String },
    #[error("{}", self.message())]
    ProjectNotFound(PathBuf),
    #[error("{}", self.message())]
    TaskNotFound(TaskId),
    #[error("{}", self.message())]
    TaskNotRunning(TaskId),
    #[error("{}", self.message())]
    TaskClosed(TaskId),
    #[error("{}", self.message())]
    TaskWithoutChanges(TaskId),
    #[error("{}", self.message())]
    NotAnOutput(String),
    #[error("{}", self.message())]
    FlowNotFound(String),
    #[error("{}", self.message())]
    InvalidFlow(Vec<String>),
    #[error("{}", self.message())]
    InvalidBlockedPattern(String),
    #[error("{}", self.message())]
    OutputNotOpened(String),
    #[error("{}", self.message())]
    OutputNotSaved(String),
    #[error("{}", self.message())]
    MissingApiKey(AccountId),
    #[error("{}", self.message())]
    NotAHandoffCandidate(String),
    #[error("{}", self.message())]
    NoteNotFound(i64),
    #[error("{}", self.message())]
    NotAProposal(i64),
    #[error("{}", self.message())]
    EmptyNote,
    #[error("{}", self.message())]
    SkillNotFound(String),
    #[error("{}", self.message())]
    SkillNotAProposal(i64),
    #[error("{}", self.message())]
    SkillNameTaken(String),
    #[error("{}", self.message())]
    GlobalSkillInProtectedProject,
    #[error("{}", self.message())]
    NoSkillsToImport(PathBuf),
    #[error("{}", self.message())]
    SkillReviewFailed(Option<String>),
    #[error("{}", self.message())]
    SkillReviewTimedOut,
    #[error("{}", self.message())]
    TrialTasks,
    #[error("{}", self.message())]
    TrialSkillOutside(std::path::PathBuf),
    #[error("{}", self.message())]
    TrialRun,
    #[error("{}", self.message())]
    SkillOutsideDuringTrial,
    #[error("{}", self.message())]
    SkillOutsideProjectOnly,
    #[error("{}", self.message())]
    EmptyPrompt,
    #[error("{}", self.message())]
    ProviderNotYetSupported(Provider),
    #[error("{}", self.message())]
    MergeBlocked {
        branch: String,
        reason: MergeOutcome,
    },
    #[error("{}", self.message())]
    UnknownSetting(String),
    #[error("{}", self.message())]
    QueuedMessageNotFound(i64),
    #[error("{}", self.message())]
    TemplateNotFound(String),
    #[error("{}", self.message())]
    TemplateExists(String),
    #[error("{}", self.message())]
    UndoBlocked(UndoBlocked),
    #[error("{}", self.message())]
    ProjectFolderExists(PathBuf),
    #[error("{}", self.message())]
    RelativeProjectParent(PathBuf),
    #[error("{}", self.message())]
    ConversationChangesFiles,
    #[error("{}", self.message())]
    NotAConversation(TaskId),
    /// The agent that would get the message takes no images.
    #[error("{}", self.message())]
    ImagesNotAccepted(String),
    /// Images cannot wait in the queue: they go only with a message sent now.
    #[error("{}", self.message())]
    ImagesWhileQueued,
    /// A path that is not a file inside the copy (absolute, `..`, `.git`...).
    #[error("{}", self.message())]
    InvalidCodePath(String),
    /// A revision git should not be handed (an option, a range...).
    #[error("{}", self.message())]
    InvalidRevision(String),
    /// The task's copy is gone (discarded, or accepted before Hedy kept a
    /// record) or it has none (a conversation).
    #[error("{}", self.message())]
    CopyUnavailable(TaskId),
    /// A protected file (`.env`, keys...): Hedy never reads it.
    #[error("{}", self.message())]
    FileLocked(String),
    /// Commit, push, pull or pull request (see [`ShipError`]).
    #[error("{}", self.message())]
    Ship(ShipError),
    /// An accepted task's snapshot: its files are read from a commit.
    #[error("{}", self.message())]
    CopyReadOnly,
    /// The task's agent is working on that copy: editing waits for it.
    #[error("{}", self.message())]
    CopyBusy(TaskId),
    /// A new file or folder, or a rename, would replace something.
    #[error("{}", self.message())]
    PathExists(String),
    /// Nothing at that path of the copy.
    #[error("{}", self.message())]
    PathMissing(String),
    /// Not a text file Hedy can edit (binary, too large, not UTF-8).
    #[error("{}", self.message())]
    FileNotEditable(String),
    /// Not a valid name for a file or folder.
    #[error("{}", self.message())]
    InvalidFileName(String),
    /// The conversation's agent is still answering: carrying waits for it.
    #[error("{}", self.message())]
    ConversationBusy(TaskId),
    /// No question kept and no summary: nothing to carry.
    #[error("{}", self.message())]
    NothingToCarry,
    /// Rule 3: the conversation's account is at its limit; no other is used.
    #[error("{}", self.message())]
    SummaryAtLimit { account: String },
    #[error("{}", self.message())]
    SummaryFailed(Option<String>),
    #[error("{}", self.message())]
    SummaryTimedOut,
    /// A question to the memory with no words.
    #[error("{}", self.message())]
    AskEmpty,
    /// Rule 3: the account that answers is at its limit; no other is used.
    #[error("{}", self.message())]
    AskAtLimit { account: String },
    /// No usable answer came back (with the agent's error, if any).
    #[error("{}", self.message())]
    AskFailed(Option<String>),
    #[error("{}", self.message())]
    AskTimedOut,
    /// Rule 4: every project of the team here is protected, so its notes
    /// only go to their own account, from that project.
    #[error("{}", self.message())]
    AskTeamProtected,
    /// Looking in the code needs Claude Code: the others read protected
    /// files without asking.
    #[error("{}", self.message())]
    AskCodeNeedsClaude { account: String },
    /// Checking the rules of a change that has none.
    #[error("{}", self.message())]
    RulesNoChanges,
    /// A range of commits that is not one (or could pass for an option).
    #[error("{}", self.message())]
    RulesBadRevs(String),
    /// No usable check came back (with the agent's error, if any).
    #[error("{}", self.message())]
    RulesFailed(Option<String>),
    #[error("{}", self.message())]
    RulesTimedOut,
    /// Rule 4: the change touches another protected project's files, whose
    /// code only goes to that project's account.
    #[error("{}", self.message())]
    RulesProtectedInside { project: String },
    #[error("{}", self.message())]
    ThemeNotFound(String),
    /// Rule 3: the chosen account is at its limit; no other is used.
    #[error("{}", self.message())]
    ThemeAtLimit { account: String },
    /// No usable theme came back (with the agent's error, if any).
    #[error("{}", self.message())]
    ThemeDescriptionFailed(Option<String>),
    #[error("{}", self.message())]
    ThemeDescriptionTimedOut,
    /// Personal sync is not set up on this PC.
    #[error("{}", self.message())]
    SyncNotSetUp,
    #[error("{}", self.message())]
    SyncAlreadySetUp,
    /// The repo already holds another sync: join it instead.
    #[error("{}", self.message())]
    SyncRepoInUse,
    /// Joining a repo where no sync was set up.
    #[error("{}", self.message())]
    SyncRepoEmpty,
    /// The newest key epoch leaves this PC out.
    #[error("{}", self.message())]
    SyncRemoved,
    /// No PC with that id or name syncs.
    #[error("{}", self.message())]
    SyncDeviceNotFound(String),
    /// The repo for a new sync already has files (not Hedy's).
    #[error("{}", self.message())]
    SyncRepoNotEmpty,
    /// Personal sync pointed at a team's repo.
    #[error("{}", self.message())]
    SyncTeamRepo,
    /// Another PC changed the sync's keys at the same moment.
    #[error("{}", self.message())]
    SyncBusy,
    /// Several PCs have that name.
    #[error("{}", self.message())]
    SyncDeviceAmbiguous(String),
    /// A PC cannot remove itself (it leaves instead).
    #[error("{}", self.message())]
    SyncRemoveThisPc,
    #[error("{}", self.message())]
    SyncConflictNotFound(i64),
    /// A key epoch this PC already checked changed in the repo.
    #[error("{}", self.message())]
    SyncTampered(u32),
    /// The sync repo's URL carries a password or a token.
    #[error("{}", self.message())]
    SyncUrlWithCredentials,
    /// This PC is in no team with that id or name.
    #[error("{}", self.message())]
    TeamNotFound(String),
    /// A team note with hidden characters (shown as `⟦U+202E⟧`).
    #[error("{}", self.message())]
    TeamHiddenText(String),
    /// No team note or suggestion with that id.
    #[error("{}", self.message())]
    TeamItemNotFound(String),
    /// The team member's role does not allow it.
    #[error("{}", self.message())]
    TeamNotAllowed,
    /// The project's memory is not this team's.
    #[error("{}", self.message())]
    TeamProjectNotLinked,
    /// What was typed is not an invite code (caught before it burns one).
    #[error("{}", self.message())]
    TeamInviteCode,
    #[error("{}", self.message())]
    TeamInviteNotFound(u32),
    #[error("{}", self.message())]
    TeamInviteExpired,
    /// Someone else already answered that invite.
    #[error("{}", self.message())]
    TeamInviteUsed,
    /// The inviting PC burned the invite: the code did not match.
    #[error("{}", self.message())]
    TeamInviteFailed,
    /// Joining a team in a repo that holds personal sync.
    #[error("{}", self.message())]
    TeamNotATeam,
    /// The change would leave the team without an admin.
    #[error("{}", self.message())]
    TeamLastAdmin,
    #[error("{}", self.message())]
    TeamPersonNotFound(String),
    /// An admin took this PC out of the team.
    #[error("{}", self.message())]
    TeamRemoved,
}

/// Why accepting a task cannot be undone safely. Hedy only moves the base
/// branch back while nothing else happened on it; otherwise the user decides
/// (for example with `git revert`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoBlocked {
    /// The task is not accepted.
    NotAccepted,
    /// Accepted before Hedy kept what the merge did.
    NoRecord,
    /// The main checkout is on another branch now.
    NotOnBase { current: String },
    /// New commits landed on the base branch after the merge.
    MovedOn,
    /// The merge is already on a remote branch.
    Published { remotes: Vec<String> },
    /// Git refused: files the merge changed have local changes.
    LocalChanges(String),
}

pub type AppResult<T> = Result<T, AppError>;

fn paths(list: &[PathBuf]) -> String {
    list.iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl AppError {
    fn message(&self) -> String {
        match self {
            Self::Domain(e) => e.to_string(),
            Self::Ship(e) => e.message(),
            Self::Port(e) => e.to_string(),
            Self::AccountNotFound(id) => tr!(
                "no existe la cuenta «{id}» (mira `hedy account list`)",
                "account «{id}» does not exist (see `hedy account list`)"
            ),
            Self::AccountExists(id) => tr!(
                "ya existe una cuenta con el identificador «{id}»",
                "an account with id «{id}» already exists"
            ),
            Self::DefaultFolderTaken(cli) => tr!(
                "ya hay una cuenta que usa la carpeta por defecto de {cli}",
                "another account already uses the default folder of {cli}"
            ),
            Self::AccountInUse { account, projects } => {
                let projects = paths(projects);
                tr!(
                    "la cuenta «{account}» está asignada a {projects}; asígnales otra antes de borrarla",
                    "account «{account}» is assigned to {projects}; assign them another one before removing it"
                )
            }
            Self::NoAccountResolved => tr!(
                "este proyecto no tiene cuenta y no hay cuenta por defecto (usa `hedy use <cuenta>`)",
                "this project has no account and there is no default account (use `hedy use <account>`)"
            ),
            Self::NotASubscription(id) => tr!(
                "la cuenta «{id}» es por API key: no tiene login de CLI",
                "account «{id}» uses an API key: it has no CLI login"
            ),
            Self::NotAnApiAccount(id) => tr!(
                "la cuenta «{id}» es de suscripción: no usa API key",
                "account «{id}» is a subscription: it uses no API key"
            ),
            Self::EmptyApiKey => tr!("la API key está vacía", "the API key is empty"),
            Self::DefaultCannotBeProtected(id) => tr!(
                "«{id}» es la cuenta por defecto y no se puede reservar para un proyecto protegido; elige otra cuenta por defecto (`hedy account default <cuenta>`) o quita la protección (`--unprotect`)",
                "«{id}» is the default account and cannot be reserved for a protected project; pick another default (`hedy account default <account>`) or remove the protection (`--unprotect`)"
            ),
            Self::InvalidSettingValue { key, value } => tr!(
                "valor no válido para «{key}»: «{value}»",
                "invalid value for «{key}»: «{value}»"
            ),
            Self::ProjectNotFound(path) => {
                let path = path.display();
                tr!(
                    "no existe el proyecto {path}",
                    "project {path} does not exist"
                )
            }
            Self::TaskNotFound(id) => tr!("no existe la tarea {id}", "task {id} does not exist"),
            Self::TaskNotRunning(id) => tr!(
                "la tarea {id} no tiene un agente en marcha",
                "task {id} has no agent running"
            ),
            Self::TaskClosed(id) => tr!(
                "la tarea {id} ya está cerrada",
                "task {id} is already closed"
            ),
            Self::TaskWithoutChanges(id) => tr!(
                "la tarea {id} no cambia archivos (es una pregunta)",
                "task {id} changes no files (it is a question)"
            ),
            Self::InvalidBlockedPattern(pattern) => tr!(
                "«{pattern}» no sirve: pon nombres de archivo o patrones de nombre (.env, .env.*, *.pem), sin carpetas y sin que abarquen todo",
                "«{pattern}» does not work: use file names or name patterns (.env, .env.*, *.pem), without folders and not matching everything"
            ),
            Self::FlowNotFound(name) => tr!(
                "no hay un flujo «{name}»; `hedy flow list` muestra los que hay",
                "there is no flow «{name}»; `hedy flow list` shows the ones there are"
            ),
            Self::InvalidFlow(problems) => {
                let problems = problems.join("; ");
                tr!(
                    "el flujo no se puede ejecutar: {problems}",
                    "the flow cannot run: {problems}"
                )
            }
            Self::NotAnOutput(path) => tr!(
                "«{path}» no es un archivo que escribió la tarea",
                "«{path}» is not a file the task wrote"
            ),
            Self::OutputNotOpened(path) => tr!(
                "Hedy no abre «{path}»: es un tipo de archivo que podría ejecutar algo. Guárdalo y ábrelo tú si confías en él.",
                "Hedy does not open «{path}»: that type of file could run something. Save it and open it yourself if you trust it."
            ),
            Self::OutputNotSaved(name) => tr!(
                "No se pudo guardar «{name}»: ese nombre ya existe muchas veces en la carpeta",
                "Could not save «{name}»: that name is already taken many times in the folder"
            ),
            Self::MissingApiKey(id) => tr!(
                "la cuenta «{id}» no tiene API key guardada",
                "account «{id}» has no saved API key"
            ),
            Self::NotAHandoffCandidate(id) => tr!(
                "no se puede pasar la tarea a «{id}»: elige otro proveedor o una API key",
                "the task cannot move to «{id}»: pick another provider or an API key"
            ),
            Self::NoteNotFound(id) => tr!("no existe la nota {id}", "note {id} does not exist"),
            Self::NotAProposal(id) => tr!(
                "la nota {id} no es una propuesta: ya está aprobada",
                "note {id} is not a proposal: it is already approved"
            ),
            Self::EmptyNote => tr!("la nota está vacía", "the note is empty"),
            Self::SyncNotSetUp => tr!(
                "la sincronización no está configurada en esta PC",
                "sync is not set up on this PC"
            ),
            Self::SyncAlreadySetUp => tr!(
                "esta PC ya sincroniza con un repo",
                "this PC already syncs with a repo"
            ),
            Self::SyncRepoInUse => tr!(
                "ese repo ya tiene una sincronización: elige «Ya la uso en otra PC»",
                "that repo already holds a sync: choose «I already use it on another PC»"
            ),
            Self::SyncRepoEmpty => tr!(
                "ese repo todavía no tiene una sincronización: configúrala primero en tu otra PC",
                "that repo holds no sync yet: set it up on your other PC first"
            ),
            Self::SyncRemoved => tr!(
                "esta PC se quitó de la sincronización desde otra PC: ya no puede leer lo nuevo. Tus notas siguen aquí; para volver, deja de sincronizar y súmala de nuevo con tus 12 palabras",
                "this PC was removed from sync on another PC: it can no longer read what is new. Your notes stay here; to come back, stop syncing and add it again with your 12 words"
            ),
            Self::SyncDeviceNotFound(name) => tr!(
                "no hay ninguna PC «{name}» en la sincronización",
                "no PC \"{name}\" syncs"
            ),
            Self::SyncRepoNotEmpty => tr!(
                "ese repo ya tiene archivos: usa uno vacío, creado sin README",
                "that repo already has files: use an empty one, created without a README"
            ),
            Self::SyncTeamRepo => tr!(
                "ese repo es de un equipo: únete a él con una invitación",
                "that repo belongs to a team: join it with an invite"
            ),
            Self::SyncBusy => tr!(
                "otra PC acaba de cambiar las claves de la sincronización; vuelve a intentarlo",
                "another PC just changed the sync's keys; try again"
            ),
            Self::SyncDeviceAmbiguous(name) => tr!(
                "hay varias PCs llamadas «{name}»: usa su id (`hedy sync` lo muestra)",
                "several PCs are called \"{name}\": use its id (`hedy sync` shows it)"
            ),
            Self::SyncRemoveThisPc => tr!(
                "esta PC no se quita a sí misma: deja de sincronizar en ella, y quítala desde otra",
                "this PC does not remove itself: stop syncing on it, and remove it from another"
            ),
            Self::SyncUrlWithCredentials => tr!(
                "esa dirección lleva una contraseña o un token, y Hedy no guarda ninguno: usa la dirección sin él (con SSH, o con el gestor de credenciales de git)",
                "that URL carries a password or a token, and Hedy keeps none: use the URL without it (with SSH, or with git's credential helper)"
            ),
            Self::SyncTampered(epoch) => tr!(
                "el repo de sincronización fue alterado: la época de llaves {epoch}, que esta PC ya había verificado, cambió. No se sincroniza hasta revisarlo",
                "the sync repo was tampered with: key epoch {epoch}, which this PC had already checked, changed. Sync stops until it is looked at"
            ),
            Self::TeamNotFound(team) => tr!(
                "esta PC no está en el equipo «{team}»",
                "this PC is not in the team \"{team}\""
            ),
            Self::TeamHiddenText(shown) => tr!(
                "la nota tiene caracteres ocultos que el agente leería y tú no ves: {shown}. Escríbela de nuevo sin ellos",
                "the note has hidden characters the agent would read and you do not see: {shown}. Write it again without them"
            ),
            Self::TeamItemNotFound(uid) => tr!(
                "el equipo no tiene la nota o sugerencia {uid}",
                "the team has no note or suggestion {uid}"
            ),
            Self::TeamNotAllowed => tr!(
                "tu rol en el equipo no lo permite: pídeselo a un admin",
                "your role in the team does not allow it: ask an admin"
            ),
            Self::TeamProjectNotLinked => tr!(
                "la memoria de este proyecto no es del equipo: conéctalo al equipo primero",
                "this project's memory is not the team's: connect it to the team first"
            ),
            Self::TeamInviteCode => tr!(
                "eso no es un código de invitación: revisa el número y las cuatro palabras",
                "that is not an invite code: check the number and the four words"
            ),
            Self::TeamInviteNotFound(number) => tr!(
                "el equipo no tiene la invitación {number}: puede que ya se usara o se cancelara",
                "the team has no invite {number}: it may have been used or cancelled"
            ),
            Self::TeamInviteExpired => tr!(
                "la invitación venció: pide otra",
                "the invite expired: ask for another"
            ),
            Self::TeamInviteUsed => tr!(
                "alguien ya usó esa invitación: pide otra",
                "someone already used that invite: ask for another"
            ),
            Self::TeamInviteFailed => tr!(
                "la invitación no funcionó (el código no coincidió) y ya no sirve: pide otra",
                "the invite did not work (the code did not match) and is no longer valid: ask for another"
            ),
            Self::TeamRemoved => tr!(
                "esta PC ya no está en el equipo: un admin la quitó, y no puede leer lo nuevo. Lo que ya tenía sigue aquí; sal con `hedy team leave` (o desde Ajustes) y, para volver, pide otra invitación",
                "this PC is no longer in the team: an admin removed it, and it cannot read what is new. What it had stays here; leave with `hedy team leave` (or from Settings) and, to come back, ask for another invite"
            ),
            Self::TeamLastAdmin => tr!(
                "el equipo se quedaría sin admin: haz admin a otra persona primero",
                "the team would be left without an admin: make someone else an admin first"
            ),
            Self::TeamPersonNotFound(person) => tr!(
                "nadie del equipo es «{person}»",
                "nobody in the team is \"{person}\""
            ),
            Self::TeamNotATeam => tr!("ese repo no es de un equipo", "that repo is not a team's"),
            Self::SyncConflictNotFound(id) => tr!(
                "no existe el conflicto {id}",
                "conflict {id} does not exist"
            ),
            Self::SkillNotFound(skill) => tr!(
                "no existe la habilidad «{skill}» en este proyecto",
                "there is no skill «{skill}» in this project"
            ),
            Self::SkillNotAProposal(id) => tr!(
                "la habilidad {id} no es una propuesta: ya está aprobada",
                "skill {id} is not a proposal: it is already approved"
            ),
            Self::SkillNameTaken(name) => tr!(
                "ya hay otra habilidad aprobada llamada «{name}»",
                "another approved skill is already called «{name}»"
            ),
            Self::GlobalSkillInProtectedProject => tr!(
                "un proyecto protegido no comparte habilidades globales: déjala en el proyecto",
                "a protected project does not share global skills: keep it in the project"
            ),
            Self::SkillReviewFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo revisar la habilidad: {detail}",
                    "could not review the skill: {detail}"
                ),
                None => tr!(
                    "la IA no devolvió la revisión; vuelve a intentarlo",
                    "the AI gave no review; try again"
                ),
            },
            Self::TrialTasks => tr!(
                "elige de 1 a 3 tareas para probar la habilidad",
                "pick 1 to 3 tasks to try the skill on"
            ),
            Self::TrialSkillOutside(path) => {
                let path = path.display();
                tr!(
                    "la habilidad también está en {path}, así que la ejecución sin ella la vería; desactiva «Usar también fuera de Hedy» o quítala de ahí para probarla",
                    "the skill is also at {path}, so the run without it would see it; turn off «Use outside Hedy too» or remove it from there to try it"
                )
            }
            Self::SkillOutsideProjectOnly => tr!(
                "solo una habilidad aprobada de un proyecto se puede usar también fuera de Hedy",
                "only an approved project skill can be used outside Hedy too"
            ),
            Self::TrialRun => tr!(
                "es una ejecución de prueba de una habilidad: no se acepta, ni se continúa, ni pasa a otra cuenta; se descarta sola al terminar",
                "it is a trial run of a skill: it cannot be accepted, continued or moved to another account; it is discarded on its own when it ends"
            ),
            Self::SkillOutsideDuringTrial => tr!(
                "se está probando esta habilidad: la ejecución sin ella la vería; espera a que termine la prueba",
                "this skill is being tried: the run without it would see it; wait until the trial ends"
            ),
            Self::SkillReviewTimedOut => tr!(
                "la IA tardó demasiado en revisar la habilidad; vuelve a intentarlo",
                "the AI took too long to review the skill; try again"
            ),
            Self::NoSkillsToImport(path) => {
                let path = path.display();
                tr!(
                    "no hay habilidades en {path}: elige un SKILL.md, la carpeta de una habilidad o una carpeta de habilidades (como .claude/skills)",
                    "there are no skills in {path}: pick a SKILL.md, a skill's folder or a folder of skills (such as .claude/skills)"
                )
            }
            Self::EmptyPrompt => tr!("escribe qué quieres que haga", "write what you want done"),
            Self::ProviderNotYetSupported(provider) => tr!(
                "las tareas con {provider} todavía no están disponibles en Hedy",
                "tasks with {provider} are not available in Hedy yet"
            ),
            Self::MergeBlocked { branch, reason } => {
                let why = match reason {
                    MergeOutcome::NotOnBase { current } => tr!(
                        "tu carpeta principal está en la rama «{current}», no en la rama base",
                        "your main folder is on branch «{current}», not on the base branch"
                    ),
                    MergeOutcome::Dirty => tr!(
                        "tu carpeta principal tiene cambios sin guardar en git",
                        "your main folder has uncommitted changes"
                    ),
                    MergeOutcome::Conflict => {
                        tr!(
                            "hay conflictos con la rama base",
                            "it conflicts with the base branch"
                        )
                    }
                    MergeOutcome::Merged => tr!("ya está unida", "it is already merged"),
                };
                tr!(
                    "Los cambios quedaron guardados en la rama «{branch}», pero no se pudieron unir: {why}. Resuélvelo y vuelve a aceptar, o únela tú con git.",
                    "The changes are saved on branch «{branch}», but could not be merged: {why}. Fix it and accept again, or merge it yourself with git."
                )
            }
            Self::UnknownSetting(key) => {
                tr!("ajuste desconocido: «{key}»", "unknown setting: «{key}»")
            }
            Self::QueuedMessageNotFound(_) => tr!(
                "ese mensaje ya no está en cola: puede que ya se haya enviado",
                "that message is no longer queued: it may have been sent already"
            ),
            Self::TemplateNotFound(name) => tr!(
                "no existe la plantilla «{name}» (mira `hedy template list`)",
                "template «{name}» does not exist (see `hedy template list`)"
            ),
            Self::TemplateExists(name) => tr!(
                "ya hay una plantilla llamada «{name}» en ese alcance",
                "there is already a template called «{name}» in that scope"
            ),
            Self::UndoBlocked(why) => {
                let why = match why {
                    UndoBlocked::NotAccepted => {
                        tr!("la tarea no está integrada", "the task is not merged")
                    }
                    UndoBlocked::NoRecord => tr!(
                        "se integró antes de que Hedy guardara cómo deshacerlo",
                        "it was merged before Hedy kept how to undo it"
                    ),
                    UndoBlocked::NotOnBase { current } => tr!(
                        "tu carpeta principal está ahora en la rama «{current}»",
                        "your main folder is now on branch «{current}»"
                    ),
                    UndoBlocked::MovedOn => tr!(
                        "ya hay commits nuevos después de la integración",
                        "there are new commits after the merge"
                    ),
                    UndoBlocked::Published { remotes } => {
                        let remotes = remotes.join(", ");
                        tr!(
                            "los cambios ya se subieron a {remotes}",
                            "the changes were already pushed to {remotes}"
                        )
                    }
                    UndoBlocked::LocalChanges(git) => tr!(
                        "hay cambios sin guardar en archivos que tocó la integración ({git})",
                        "files the merge touched have uncommitted changes ({git})"
                    ),
                };
                tr!(
                    "No se puede deshacer sin riesgo: {why}. Si quieres quitar los cambios, usa git revert.",
                    "It cannot be undone safely: {why}. To take the changes out, use git revert."
                )
            }
            Self::ProjectFolderExists(path) => {
                let path = path.display();
                tr!(
                    "ya existe {path}; elige otro nombre o ábrela con «Abrir carpeta…»",
                    "{path} already exists; pick another name or open it with “Open folder…”"
                )
            }
            Self::RelativeProjectParent(path) => {
                let path = path.display();
                tr!(
                    "la carpeta donde crear el proyecto debe ser una ruta completa: {path}",
                    "the folder to create the project in must be a full path: {path}"
                )
            }
            Self::ConversationChangesFiles => tr!(
                "una pregunta rápida solo pregunta o investiga; para cambiar archivos, crea o abre un proyecto",
                "a quick question only asks or researches; to change files, create or open a project"
            ),
            Self::NotAConversation(id) => tr!(
                "la tarea {id} es de un proyecto, no una conversación",
                "task {id} belongs to a project, it is not a conversation"
            ),
            Self::ImagesNotAccepted(agent) => tr!(
                "{agent} no acepta imágenes con esta cuenta: quítalas o usa otra cuenta",
                "{agent} does not take images with this account: remove them or use another account"
            ),
            Self::ImagesWhileQueued => tr!(
                "las imágenes no pueden esperar en la cola: envíalas cuando el agente termine su turno",
                "images cannot wait in the queue: send them when the agent finishes its turn"
            ),
            Self::InvalidCodePath(path) => tr!(
                "«{path}» no es un archivo dentro de esta copia",
                "«{path}» is not a file inside this copy"
            ),
            Self::InvalidRevision(rev) => tr!(
                "«{rev}» no es un commit válido",
                "«{rev}» is not a valid commit"
            ),
            Self::CopyUnavailable(id) => tr!(
                "los archivos de la tarea {id} ya no están disponibles (se descartó o se integró antes de que Hedy guardara su commit)",
                "the files of task {id} are no longer available (it was discarded, or accepted before Hedy kept its commit)"
            ),
            Self::FileLocked(path) => tr!(
                "{path} está protegido: Hedy no lo lee; ábrelo en tu editor",
                "{path} is protected: Hedy does not read it; open it in your editor"
            ),
            Self::CopyReadOnly => tr!(
                "es una instantánea de solo lectura (la tarea ya se integró): edita en tu carpeta",
                "this is a read-only snapshot (the task was already accepted): edit in your folder"
            ),
            Self::CopyBusy(id) => tr!(
                "la IA está trabajando en la copia de la tarea {id}; podrás editar cuando termine",
                "the AI is working on task {id}'s copy; you can edit when it finishes"
            ),
            Self::PathExists(path) => tr!("«{path}» ya existe", "«{path}» already exists"),
            Self::PathMissing(path) => tr!("«{path}» no existe", "«{path}» does not exist"),
            Self::FileNotEditable(path) => tr!(
                "{path} no se puede editar en Hedy (binario, muy grande o no es UTF-8): ábrelo en tu editor",
                "{path} cannot be edited in Hedy (binary, too large or not UTF-8): open it in your editor"
            ),
            Self::InvalidFileName(name) => tr!(
                "«{name}» no es un nombre válido para un archivo o carpeta",
                "«{name}» is not a valid name for a file or folder"
            ),
            Self::ConversationBusy(_) => tr!(
                "la conversación todavía está respondiendo; espera a que termine para llevarla a un proyecto",
                "the conversation is still answering; wait for it to finish before carrying it into a project"
            ),
            Self::NothingToCarry => tr!(
                "no queda nada de la conversación para llevar: conserva al menos una pregunta o un resumen",
                "nothing of the conversation is left to carry: keep at least one question or a summary"
            ),
            Self::SummaryAtLimit { account } => tr!(
                "la cuenta «{account}» está en su límite. Hedy no usa otra cuenta: lleva las preguntas tal cual o espera a que se renueve",
                "account «{account}» is at its limit. Hedy does not use another account: carry the questions as they are or wait for it to renew"
            ),
            Self::SummaryFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo resumir la conversación: {detail}",
                    "could not summarize the conversation: {detail}"
                ),
                None => tr!(
                    "no se pudo resumir la conversación; lleva las preguntas tal cual",
                    "could not summarize the conversation; carry the questions as they are"
                ),
            },
            Self::SummaryTimedOut => tr!(
                "la IA tardó demasiado en resumir; vuelve a intentarlo o lleva las preguntas tal cual",
                "the AI took too long to summarize; try again or carry the questions as they are"
            ),
            Self::AskEmpty => tr!(
                "escribe una pregunta para la memoria",
                "write a question for the memory"
            ),
            Self::AskAtLimit { account } => tr!(
                "la cuenta «{account}» está en su límite. Hedy no usa otra cuenta: elige otra tú o espera a que se renueve",
                "account «{account}» is at its limit. Hedy does not use another account: pick another one yourself or wait for it to renew"
            ),
            Self::AskFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo responder: {detail}",
                    "could not answer: {detail}"
                ),
                None => tr!(
                    "la IA no devolvió una respuesta; vuelve a intentarlo",
                    "the AI gave no answer; try again"
                ),
            },
            Self::AskCodeNeedsClaude { account } => tr!(
                "buscar en el código necesita una cuenta de Claude Code: con «{account}» la IA podría leer archivos protegidos (.env, claves) sin pedir permiso",
                "looking in the code needs a Claude Code account: with «{account}» the AI could read protected files (.env, keys) without asking"
            ),
            Self::AskTeamProtected => tr!(
                "las notas de ese equipo solo las lee la cuenta de su proyecto protegido: pregunta desde ese proyecto",
                "that team's notes are only read by its protected project's account: ask from that project"
            ),
            Self::AskTimedOut => tr!(
                "la IA tardó demasiado en responder; vuelve a intentarlo",
                "the AI took too long to answer; try again"
            ),
            Self::RulesNoChanges => tr!(
                "no hay cambios que revisar",
                "there are no changes to check"
            ),
            Self::RulesBadRevs(revs) => tr!(
                "«{revs}» no es un commit ni un rango de commits (por ejemplo, main..HEAD)",
                "«{revs}» is not a commit or a range of commits (for example, main..HEAD)"
            ),
            Self::RulesFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo revisar el cambio: {detail}",
                    "could not check the change: {detail}"
                ),
                None => tr!(
                    "la IA no devolvió la revisión; vuelve a intentarlo",
                    "the AI gave no review; try again"
                ),
            },
            Self::RulesProtectedInside { project } => tr!(
                "el cambio toca archivos del proyecto protegido «{project}», que solo lee su propia cuenta: esos archivos se revisan desde ese proyecto",
                "the change touches files of the protected project «{project}», which only its own account reads: those files are checked from that project"
            ),
            Self::RulesTimedOut => tr!(
                "la IA tardó demasiado en revisar el cambio; vuelve a intentarlo",
                "the AI took too long to check the change; try again"
            ),
            Self::ThemeNotFound(id) => {
                tr!("no existe el tema «{id}»", "theme «{id}» does not exist")
            }
            Self::ThemeAtLimit { account } => tr!(
                "la cuenta «{account}» está en su límite. Hedy no usa otra cuenta: elige otra tú, espera a que se renueve o importa un tema de VS Code",
                "account «{account}» is at its limit. Hedy does not use another account: pick another one yourself, wait for it to renew or import a VS Code theme"
            ),
            Self::ThemeDescriptionFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo crear el tema: {detail}",
                    "could not create the theme: {detail}"
                ),
                None => tr!(
                    "la IA no devolvió un tema válido; vuelve a intentarlo o describe el tema de otra forma",
                    "the AI did not return a valid theme; try again or describe the theme another way"
                ),
            },
            Self::ThemeDescriptionTimedOut => tr!(
                "la IA tardó demasiado en crear el tema; vuelve a intentarlo",
                "the AI took too long to create the theme; try again"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn messages_follow_the_language() {
        let error = AppError::EmptyNote;
        assert_eq!(
            with_language(Language::English, || error.to_string()),
            "the note is empty"
        );
        assert_eq!(error.to_string(), "la nota está vacía");
    }
}
