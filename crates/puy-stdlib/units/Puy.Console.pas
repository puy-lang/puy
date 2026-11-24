unit Puy.Console;

// Standard input, output and error as streams (docs/spec/stdlib/Puy.Console/README.md;
// docs/design/stdlib.md 3.7). Written from the specification only, not from any other
// implementation.
//
// The streams share the VM's buffers with Read / ReadLn and Write / WriteLn, so that a program
// may mix the two and the bytes still go in the order they were written and read: the output
// stream appends to the standard output buffer, which is written out by the same rule
// (stdout-buffering); the error stream writes at once, after writing out standard output; the
// input stream reads the bytes ReadLn has not read.

interface

uses
  System.SysUtils, System.Classes;

type
  // The streams of standard input, output and error: one of each per isolate.
  TConsole = class
  private
    class function GetInput: TStream; static;
    class function GetOutput: TStream; static;
    class function GetError: TStream; static;
  public
    // Reads a line of standard input as ReadLn does, from the same buffer: the line without its
    // line break (LF, and a CR before it); '' at the end of input (Eof tells it from an empty
    // line).
    class function ReadLine: string; static;
    // Reads the bytes of standard input as they come (Read may give fewer bytes than asked
    // before the end of input). It cannot be written to.
    class property Input: TStream read GetInput;
    // Writes to standard output, through the buffer of Write / WriteLn. It cannot be read.
    class property Output: TStream read GetOutput;
    // Writes to standard error at once. It cannot be read.
    class property Error: TStream read GetError;
  end;

implementation

// The streams read and write the VM's buffers with the intrinsics ConsoleRead, ConsoleWrite and
// ConsoleWriteError (docs/spec/bytecode.md 2.22); the unit has no Rust part.

type
  TConsoleKind = (ckInput, ckOutput, ckError);

  // A stream of standard input, output or error. It cannot seek; closing it does not close
  // standard input or output, only the stream.
  TConsoleStream = class(TStream)
  private
    FKind: TConsoleKind;
    FClosed: Boolean;
    procedure CheckOpen;
    function StreamName: string;
    procedure RaiseNoSeek;
  protected
    function GetSize: Int64; override;
    procedure SetSize(const NewSize: Int64); override;
    function GetCanSeek: Boolean; override;
  public
    constructor Create(AKind: TConsoleKind);
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; override;
    procedure Close; override;
  end;

var
  // The streams, made when first asked for
  Streams: array[TConsoleKind] of TStream;

function Stream(Kind: TConsoleKind): TStream;
begin
  if Streams[Kind] = nil then
    Streams[Kind] := TConsoleStream.Create(Kind);
  Result := Streams[Kind];
end;

{ TConsole }

class function TConsole.ReadLine: string;
begin
  ReadLn(Result);
end;

class function TConsole.GetInput: TStream;
begin
  Result := Stream(TConsoleKind.ckInput);
end;

class function TConsole.GetOutput: TStream;
begin
  Result := Stream(TConsoleKind.ckOutput);
end;

class function TConsole.GetError: TStream;
begin
  Result := Stream(TConsoleKind.ckError);
end;

{ TConsoleStream }

constructor TConsoleStream.Create(AKind: TConsoleKind);
begin
  inherited Create;
  FKind := AKind;
end;

procedure TConsoleStream.CheckOpen;
begin
  if FClosed then
    raise EStreamError.Create('the stream is closed');
end;

function TConsoleStream.StreamName: string;
begin
  case FKind of
    TConsoleKind.ckInput:
      Result := 'standard input';
    TConsoleKind.ckOutput:
      Result := 'standard output';
  else
    Result := 'standard error';
  end;
end;

procedure TConsoleStream.RaiseNoSeek;
begin
  CheckOpen;
  raise EStreamError.Create('the stream of ' + StreamName + ' cannot seek');
end;

function TConsoleStream.GetSize: Int64;
begin
  RaiseNoSeek;
  Result := 0;
end;

procedure TConsoleStream.SetSize(const NewSize: Int64);
begin
  RaiseNoSeek;
end;

function TConsoleStream.GetCanSeek: Boolean;
begin
  Result := False;
end;

function TConsoleStream.Read(var Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  if FKind <> TConsoleKind.ckInput then
    raise EReadError.Create('cannot read the stream of ' + StreamName);
  // Waits for the host only when no byte is left in the buffer
  Result := ConsoleRead(Buffer, Offset, Count);
end;

function TConsoleStream.Write(const Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  case FKind of
    TConsoleKind.ckOutput:
      Result := ConsoleWrite(Buffer, Offset, Count);
    TConsoleKind.ckError:
      Result := ConsoleWriteError(Buffer, Offset, Count);
  else
    raise EWriteError.Create('cannot write the stream of ' + StreamName);
  end;
end;

function TConsoleStream.Seek(const Offset: Int64; Origin: TSeekOrigin): Int64;
begin
  RaiseNoSeek;
  Result := 0;
end;

procedure TConsoleStream.Close;
begin
  FClosed := True;
end;

end.
