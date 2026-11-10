program Main;

// A multi-file example: puy.toml marks this directory as a package, and the
// units live under src/ (src/Todo/Items.pas is the unit Todo.Items).
// Run it with `puy run examples/todo/main.pas`.

uses
  Todo.Items, Todo.Report as R;

begin
  var Shopping := AddTask('Buy milk');
  AddTask('Write the report', TPriority.High);
  var Call := AddTask('Call the plumber', TPriority.Low);

  Complete(Shopping);
  Complete(Call);
  R.PrintTasks('All tasks:', Tasks);
  R.PrintTasks('Still to do:', R.Pending(Tasks));

  try
    Complete(42);
  except
    on E: ETaskNotFound do
      WriteLn('Error: ', E.Message);
  end;
end.
