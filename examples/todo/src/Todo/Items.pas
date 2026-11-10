unit Todo.Items;

// The task list: the interface section is what other units see, the
// implementation section keeps its helpers and state to itself.

interface

uses
  System.SysUtils;

type
  TPriority = (Low, Normal, High);

  TTask = record
    Id: Integer;
    Title: string;
    Priority: TPriority;
    Done: Boolean;
  end;

  ETaskNotFound = class(Exception);

function AddTask(const Title: string; Priority: TPriority = TPriority.Normal): Integer;
procedure Complete(Id: Integer);
function Tasks: TArray<TTask>;

implementation

var
  List: TArray<TTask>;
  NextId: Integer;

function IndexOf(Id: Integer): Integer;
begin
  for var I := 0 to High(List) do
    if List[I].Id = Id then
      Exit(I);
  raise ETaskNotFound.CreateFmt('no task with id %d', [Id]);
end;

function AddTask(const Title: string; Priority: TPriority): Integer;
var
  T: TTask;
begin
  T := Default(TTask);
  T.Id := NextId;
  T.Title := Title;
  T.Priority := Priority;
  List := List + [T];
  Inc(NextId);
  Result := T.Id;
end;

procedure Complete(Id: Integer);
begin
  List[IndexOf(Id)].Done := True;
end;

function Tasks: TArray<TTask>;
begin
  Result := List;
end;

initialization
  NextId := 1;
end.
