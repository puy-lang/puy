program Interfaces;

uses
  System.SysUtils;

// Interfaces: a class implements one or more interfaces, and callers work
// with the interface alone. TInterfacedObject counts references, so the
// objects below are freed when the last interface variable lets go: there
// is no Free anywhere in this program.

type
  ILogger = interface
    ['{6E1B2C4D-0F3A-4C55-9A7E-1D2B3C4D5E6F}']
    procedure Log(const Msg: string);
  end;

  ICounter = interface
    ['{8A0C1E2F-3B4D-4E6F-8091-A2B3C4D5E6F7}']
    function GetCount: Integer;
    property Count: Integer read GetCount;
  end;

  TConsoleLogger = class(TInterfacedObject, ILogger)
  public
    procedure Log(const Msg: string);
    destructor Destroy; override;
  end;

  // Wraps another logger: adds a prefix and counts the lines.
  TPrefixLogger = class(TInterfacedObject, ILogger, ICounter)
  private
    FInner: ILogger;
    FPrefix: string;
    FCount: Integer;
    function GetCount: Integer;
  public
    constructor Create(const Inner: ILogger; const Prefix: string);
    destructor Destroy; override;
    procedure Log(const Msg: string);
  end;

procedure TConsoleLogger.Log(const Msg: string);
begin
  WriteLn(Msg);
end;

destructor TConsoleLogger.Destroy;
begin
  WriteLn('(console logger freed)');
  inherited;
end;

constructor TPrefixLogger.Create(const Inner: ILogger; const Prefix: string);
begin
  inherited Create;
  FInner := Inner;
  FPrefix := Prefix;
end;

destructor TPrefixLogger.Destroy;
begin
  WriteLn('(', FPrefix, ' logger freed after ', FCount, ' lines)');
  inherited;
end;

procedure TPrefixLogger.Log(const Msg: string);
begin
  Inc(FCount);
  FInner.Log(Format('[%s] %s', [FPrefix, Msg]));
end;

function TPrefixLogger.GetCount: Integer;
begin
  Result := FCount;
end;

procedure Report(const Logger: ILogger);
var
  Counter: ICounter;
begin
  // Supports asks whether the object behind the interface also has another one.
  if Supports(Logger, ICounter, Counter) then
    WriteLn('  ', Counter.Count, ' lines so far')
  else
    WriteLn('  (this logger does not count)');
end;

// as converts to another interface and raises EInvalidCast when the object
// does not implement it. Its result is a temporary that holds a reference
// until the routine ends, which is one reason to keep it in a small function.
function LineCount(const Logger: ILogger): Integer;
begin
  Result := (Logger as ICounter).Count;
end;

procedure Run;
var
  Console, App, Db: ILogger;
begin
  Console := TConsoleLogger.Create;
  App := TPrefixLogger.Create(Console, 'app');
  Db := TPrefixLogger.Create(Console, 'db');
  // Both prefix loggers hold the console logger, so letting go of this
  // variable does not free it.
  Console := nil;

  App.Log('starting');
  Db.Log('connected');
  App.Log('ready');
  Report(App);
  Report(Db);

  WriteLn('App is ICounter: ', App is ICounter);
  WriteLn('lines through db: ', LineCount(Db));

  Db := nil; // the last reference: the db logger is freed here
  App.Log('stopping');
end; // App goes out of scope: the app logger, then the console logger

begin
  Run;
  WriteLn('done');
end.
