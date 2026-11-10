unit Todo.Report;

// Printing the task list. This unit uses Todo.Items in its interface
// section, because its routines take the types declared there.

interface

uses
  Todo.Items;

procedure PrintTasks(const Title: string; const Items: array of TTask);
function Pending(const Items: array of TTask): TArray<TTask>;

implementation

uses
  System.SysUtils;

// Not in the interface section, so other units cannot see these.
const
  Marks: array[Boolean] of string = ('[ ]', '[x]');
  PriorityNames: array[TPriority] of string = ('low', 'normal', 'high');

procedure PrintTasks(const Title: string; const Items: array of TTask);
begin
  WriteLn(Title);
  for var T in Items do
    WriteLn(Format('  %s %d. %-20s %s', [Marks[T.Done], T.Id, T.Title,
      PriorityNames[T.Priority]]));
end;

function Pending(const Items: array of TTask): TArray<TTask>;
begin
  Result := [];
  for var T in Items do
    if not T.Done then
      Result := Result + [T];
end;

end.
